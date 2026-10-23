//! Live preview, on by default as in Fusion: a model-picking dialog (Fillet, Chamfer, Shell, …)
//! previews as soon as it has a selection and again whenever a pick or value changes. A viewport
//! click withdraws the preview and goes on to pick on the un-previewed model, and bodies a preview
//! adds stay out of the browser until OK.

use fox_engine::kernel::{EdgeInfo, FaceInfo};
use fox_engine::model::{BodyId, FeatureId};
use fox_engine::scene::Pick;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind};

#[derive(Debug, Clone)]
pub(super) struct Live {
  /// The Preview checkbox.
  pub(super) on: bool,
  /// The command the last preview tried: a failed one isn't retried until something changes.
  tried: Option<Value>,
  /// The bodies before the preview.
  before: Vec<BodyId>,
  /// The picked faces and edges as they were before the preview, to find them again on it.
  picked: Vec<Was>,
  /// The features before the preview: the ones it adds stay out of the timeline until OK.
  before_features: Vec<FeatureId>,
  /// The browser's body rows before the preview (id, name): it keeps them until OK.
  before_rows: Vec<(BodyId, String)>,
  /// Preview was unticked where the session isn't at hand: withdraw it after the frame's rows.
  pub(super) withdraw: bool,
}

impl Default for Live {
  fn default() -> Live {
    Live {
      on: true,
      tried: None,
      before: Vec::new(),
      picked: Vec::new(),
      before_features: Vec::new(),
      before_rows: Vec::new(),
      withdraw: false,
    }
  }
}

impl Live {
  /// Preview ticked again: the command previews even if it is the one tried last.
  pub(super) fn retry(&mut self) {
    self.tried = None;
  }

  /// Fusion's mesh dialogs open with Preview unticked.
  pub(super) fn for_kind(kind: Kind) -> Live {
    let off = match kind {
      Kind::Form(f) => crate::forms::preview_off(f),
      Kind::PlaneCut => true,
      _ => false,
    };
    Live { on: !off, ..Live::default() }
  }
}

/// A picked edge or face of the un-previewed model.
#[derive(Debug, Clone, Copy)]
enum Was {
  Edge(BodyId, EdgeInfo),
  Face(BodyId, FaceInfo),
  /// A face Draft tilts: on the preview it is the face of its kind nearest to where it was,
  /// turned by less than 60°.
  Drafted(BodyId, FaceInfo),
}

/// Is `x` the edge `e` was, on a body a preview may have changed: the same curve, within `tol`?
/// (A preview may shorten the edge, as a fillet at its end does.)
fn same_edge(e: &EdgeInfo, x: &EdgeInfo, tol: f64) -> bool {
  use fox_engine::kernel::CurveKind;
  let dir = (e.end - e.start).normalize_or_zero();
  x.kind == e.kind
    && match e.kind {
      CurveKind::Line => {
        (x.end - x.start).normalize_or_zero().cross(dir).length() < 1e-6 && (x.mid - e.mid).cross(dir).length() < tol
      }
      CurveKind::Circle => {
        (x.center - e.center).length() < tol
          && (x.radius - e.radius).abs() < tol
          && x.axis.cross(e.axis).length() < 1e-6
      }
      _ => (x.mid - e.mid).length() < tol,
    }
}

/// Is `x` the face `f` was: the same surface, within `tol`? (A preview may trim the face, as a
/// fillet along its edge does.)
fn same_face(f: &FaceInfo, x: &FaceInfo, tol: f64) -> bool {
  x.kind == f.kind
    && x.normal.dot(f.normal) > 1.0 - 1e-9
    && if f.kind == fox_engine::kernel::SurfaceKind::Plane {
      (x.origin - f.origin).dot(f.normal).abs() < tol
    } else {
      (x.origin - f.origin).cross(f.normal).length() < tol && (x.radius - f.radius).abs() < tol
    }
}

/// The pick `was` names on the model now, if it is still there.
fn find_now(s: &Session, was: Was) -> Option<Pick> {
  match was {
    Was::Edge(body, e) => {
      let b = s.model.body(body)?;
      let tol = 1e-6 * (1.0 + b.diagonal());
      b.edges()
        .iter()
        .filter(|x| same_edge(&e, x, tol))
        .min_by(|x, y| (x.mid - e.mid).length().total_cmp(&(y.mid - e.mid).length()))
        .map(|x| Pick::Edge { body, edge: x.id })
    }
    Was::Face(body, f) => {
      let b = s.model.body(body)?;
      let tol = 1e-6 * (1.0 + b.diagonal());
      b.faces()
        .iter()
        .filter(|x| same_face(&f, x, tol))
        .min_by(|x, y| (x.center - f.center).length().total_cmp(&(y.center - f.center).length()))
        .map(|x| Pick::Face { body, face: x.id })
    }
    Was::Drafted(body, f) => find_now(s, Was::Face(body, f)).or_else(|| {
      s.model
        .body(body)?
        .faces()
        .iter()
        .filter(|x| x.kind == f.kind && x.normal.dot(f.normal) > 0.5)
        .min_by(|x, y| (x.center - f.center).length().total_cmp(&(y.center - f.center).length()))
        .map(|x| Pick::Face { body, face: x.id })
    }),
  }
}

/// The dialogs whose picked faces and edges stay selected on their preview.
fn keeps_picks(kind: Kind) -> bool {
  matches!(kind, Kind::Fillet | Kind::Chamfer | Kind::Shell | Kind::PressPull | Kind::OffsetFace | Kind::Draft)
}

impl Dialog {
  /// Asks for a preview when live preview is on and the command changed since the last one.
  pub(super) fn live_check(&mut self) {
    if !self.live.on || !self.picks() || self.picks_fixed() || self.dirty || !self.ready() || self.waits_for_value() {
      return;
    }
    let cmd = self.command().map(|(c, p)| json!([c, p]));
    if cmd.is_some() && cmd != self.live.tried {
      self.dirty = true;
    }
  }

  /// Notes what a preview is about to apply, and the bodies before it.
  pub(super) fn live_applying(&mut self, s: &Session, cmd: &str, params: &Value) {
    // the drag arrow sits on the picks of the model without the preview
    self.refresh_manip(s);
    self.live.tried = Some(json!([cmd, params]));
    self.live.before = s.model.bodies.iter().map(|b| b.id).collect();
    self.live.before_rows = s.model.bodies.iter().map(|b| (b.id, s.body_name(b.id))).collect();
    self.live.picked.clear();
    if keeps_picks(self.kind) && !self.picks_fixed() {
      let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
      // (Draft's pull face stays picked too)
      let pull = (self.kind == Kind::Draft).then_some(&self.plane);
      let n = self.faces.len();
      for (i, f) in self.faces.iter().chain(pull).enumerate() {
        if let (Some(b), Some(id)) = (get(f, "body"), get(f, "face"))
          && let Some(x) = s.model.body(BodyId(b)).and_then(|x| x.faces().iter().find(|x| x.id == id))
        {
          // Draft's faces to draft tilt under the preview; its pull face stays put
          let drafted = self.kind == Kind::Draft && i < n;
          self.live.picked.push(if drafted { Was::Drafted(BodyId(b), *x) } else { Was::Face(BodyId(b), *x) });
        }
      }
      for e in &self.edges {
        if let (Some(b), Some(id)) = (get(e, "body"), get(e, "edge"))
          && let Some(x) = s.model.body(BodyId(b)).and_then(|x| x.edges().iter().find(|x| x.id == id))
        {
          self.live.picked.push(Was::Edge(BodyId(b), *x));
        }
      }
    }
    self.live.before_features = s.design.features.iter().map(|f| f.id).collect();
  }

  /// After a preview, the picked faces (and the picked edges it kept) are selected again on the
  /// previewed body, so only they draw in the selection colour and the body keeps its own
  /// appearance, as in Fusion. (The session drops picks on a body a feature changed.)
  pub(super) fn reselect_picks(&self, s: &mut Session) {
    if !keeps_picks(self.kind) || self.picks_fixed() {
      return;
    }
    s.selection = self
      .live
      .picked
      .iter()
      .filter_map(|w| find_now(s, *w))
      .filter_map(|p| match p {
        Pick::Face { body, face } => Some(Sel::Face { body, face }),
        Pick::Edge { body, edge } => Some(Sel::Edge { body, edge }),
        _ => None,
      })
      .collect();
  }

  /// The Preview checkbox of a model-picking dialog: only Fold, Convert to Sheet Metal, Rip and
  /// Corner Closure have one, shown once something is picked (they open with their one selection
  /// row). Fusion's other command dialogs (Extrude, Fillet, Chamfer, Hole, Combine, Draft, Offset
  /// Face, Replace Face, Split Face, Pattern on Path, Replace with Primitives, ...) end with the (i)
  /// and OK / Cancel: they always preview.
  pub(super) fn live_checkbox(&mut self, ui: &mut egui::Ui, s: &mut Session) {
    if !self.has_preview_box() {
      return;
    }
    let picked = match self.kind {
      Kind::Form(f) => self.form.missing(f).is_none(),
      _ => !self.faces.is_empty() || !self.bodies.is_empty() || !self.edges.is_empty(),
    };
    if !picked {
      return;
    }
    if ui.checkbox(&mut self.live.on, "Preview").changed() {
      if self.live.on {
        self.live.tried = None;
      } else {
        self.withdraw(s);
      }
    }
  }

  /// Is this one of the dialogs with a Preview check box (see [`Self::live_checkbox`])?
  pub(super) fn has_preview_box(&self) -> bool {
    matches!(self.kind, Kind::Fold | Kind::ConvertSheet | Kind::Rip)
      || matches!(self.kind, Kind::Form(f) if std::ptr::eq(f, &crate::forms::CORNER_CLOSURE))
  }

  /// Bodies the showing preview added (the browser leaves them out until OK).
  pub fn preview_bodies(&self, s: &Session) -> Vec<BodyId> {
    if !self.applied || self.editing.is_some() {
      return Vec::new();
    }
    s.model.bodies.iter().map(|b| b.id).filter(|b| !self.live.before.contains(b)).collect()
  }

  /// The browser's body rows while a preview shows: the bodies as they were before it, so a body
  /// the preview consumes (Combine's tools) keeps its row and one it adds has none until OK, as
  /// in Fusion. None when no preview shows.
  pub fn browser_bodies(&self, s: &Session) -> Option<Vec<(BodyId, String)>> {
    if !self.applied || self.editing.is_some() {
      return None;
    }
    let name = |id: BodyId, was: &str| if s.model.body(id).is_some() { s.body_name(id) } else { was.to_string() };
    Some(self.live.before_rows.iter().map(|(id, was)| (*id, name(*id, was))).collect())
  }

  /// Features the showing preview added, in timeline order (the timeline leaves them out until
  /// OK, as in Fusion, where a feature's icon appears only when its command finishes).
  pub fn preview_features(&self, s: &Session) -> Vec<FeatureId> {
    if !self.applied || self.editing.is_some() {
      return Vec::new();
    }
    s.design.features.iter().map(|f| f.id).filter(|f| !self.live.before_features.contains(f)).collect()
  }

  /// Withdraws a model-picking dialog's preview for a viewport click, and finds what was under
  /// the cursor (`hover`, picked on the preview) on the un-previewed model: the same edge or
  /// face, when the preview didn't change it. None when there is nothing to pick there.
  pub fn unpreview_pick(&mut self, s: &mut Session, hover: Option<Pick>) -> Option<Pick> {
    let was = hover.and_then(|h| match h {
      Pick::Edge { body, edge } => {
        s.model.body(body)?.edges().iter().find(|e| e.id == edge).map(|e| Was::Edge(body, *e))
      }
      Pick::Face { body, face } => {
        s.model.body(body)?.faces().iter().find(|f| f.id == face).map(|f| Was::Face(body, *f))
      }
      _ => None,
    });
    self.withdraw(s);
    self.live.tried = None;
    find_now(s, was?)
  }
}

impl Dialog {
  /// Lets the same command preview again (after picks were put back on the un-previewed model).
  pub(super) fn live_reset(&mut self) {
    self.live.tried = None;
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use fox_engine::model::FeatureKind;
  use fox_engine::scene::Pick;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  fn block() -> Session {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
    s
  }

  fn top(s: &Session) -> Sel {
    let b = &s.model.bodies[0];
    Sel::Face { body: b.id, face: b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id }
  }

  fn edge_at(s: &Session, x: f64, y: f64, z: f64) -> Sel {
    let b = &s.model.bodies[0];
    let p = fox_engine::kernel::glam::DVec3::new(x, y, z);
    Sel::Edge { body: b.id, edge: b.edges().iter().find(|e| (e.mid - p).length() < 1e-6).unwrap().id }
  }

  /// One frame of the dialog: picks from the selection, then a live preview when due.
  fn frame(s: &mut Session, d: &mut Dialog) {
    d.sync_selection(s);
    d.live_check();
    if d.dirty {
      d.preview(s);
    }
  }

  fn edge_count(s: &Session) -> usize {
    let FeatureKind::Fillet { edges, sets, .. } = &s.design.features.last().unwrap().kind else { panic!() };
    edges.len() + sets.iter().map(|x| x.edges.len()).sum::<usize>()
  }

  /// A preview draws the body in its own appearance with only the picked faces (and the edges it
  /// kept) selected on it, as in Fusion: the session drops the picks when the body changes, and
  /// the dialog finds them again on the previewed body.
  #[test]
  fn picked_faces_stay_selected_on_the_preview() {
    let mut s = block();
    let body = s.model.bodies[0].id;
    let top_face = top(&s);
    // Shell: the opened top face is still a face of the hollowed body
    let mut d = Dialog::blank(Kind::Shell);
    s.selection = vec![top_face];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let Some(Sel::Face { body: b, face }) = s.selection.first().copied() else { panic!("{:?}", s.selection) };
    assert_eq!((b, s.selection.len()), (body, 1));
    let f = s.model.body(body).unwrap().faces().iter().find(|f| f.id == face).unwrap();
    assert!(f.normal.z > 0.9 && (f.origin.z - 20.0).abs() < 1e-6, "the top face on the previewed body: {f:?}");
    // withdrawn, the original pick is back
    d.cancel(&mut s);
    assert_eq!(s.selection, vec![top_face]);
    // Fillet by a face: the face (trimmed by the rounds) stays selected; by an edge, nothing is
    // (the edge is gone)
    let mut d = Dialog::blank(Kind::Fillet);
    s.selection = vec![top_face];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(matches!(s.selection.as_slice(), [Sel::Face { body: b, .. }] if *b == body), "{:?}", s.selection);
    d.cancel(&mut s);
    let mut d = Dialog::blank(Kind::Fillet);
    s.selection = vec![edge_at(&s, 10.0, 0.0, 20.0)];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(s.selection.is_empty(), "{:?}", s.selection);
    d.cancel(&mut s);
    // a dialog that doesn't keep its picks leaves the selection to the session
    let mut d = Dialog::blank(Kind::Draft);
    assert!(!super::keeps_picks(Kind::Combine) && super::keeps_picks(d.kind));
    d.faces = vec![json!({"body": body.0, "face": 1})];
    d.editing = Some(fox_engine::model::FeatureId(1));
    s.selection = vec![top_face];
    d.reselect_picks(&mut s);
    assert_eq!(s.selection, vec![top_face], "editing: the selection is left alone");
  }

  #[test]
  fn fillet_previews_live_and_takes_faces() {
    let mut s = block();
    let mut d = Dialog::blank(Kind::Fillet);
    frame(&mut s, &mut d);
    assert!(!d.applied, "nothing picked: no preview");
    s.selection = vec![top(&s)];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert_eq!(edge_count(&s), 4, "a face rounds its four edges");
    assert_eq!(s.design.features.len(), 2);
    // a value change previews again in place (still one feature)
    d.radius = "3 mm".into();
    frame(&mut s, &mut d);
    assert_eq!(s.design.features.len(), 2);
    assert!(d.applied);
    // a failing preview isn't retried every frame
    d.radius = "30 mm".into();
    frame(&mut s, &mut d);
    let failed = s.revision;
    frame(&mut s, &mut d);
    assert_eq!(s.revision, failed);
    // unchecking Preview withdraws it; nothing previews while off
    d.radius = "2 mm".into();
    d.live.on = false;
    d.withdraw(&mut s);
    frame(&mut s, &mut d);
    assert!(!d.applied && s.design.features.len() == 1);
  }

  #[test]
  fn viewport_picks_on_the_model_without_the_preview() {
    let mut s = block();
    let mut d = Dialog::blank(Kind::Fillet);
    s.selection = vec![edge_at(&s, 10.0, 0.0, 20.0)];
    frame(&mut s, &mut d);
    assert!(d.previewing_model());
    // a bottom edge the preview left alone, and the top face it trimmed, hovered on the preview
    let b = &s.model.bodies[0];
    let bottom = b
      .edges()
      .iter()
      .find(|e| (e.mid - fox_engine::kernel::glam::DVec3::new(10.0, 20.0, 0.0)).length() < 1e-6)
      .unwrap();
    let hover = Pick::Edge { body: b.id, edge: bottom.id };
    let picked = d.unpreview_pick(&mut s, Some(hover));
    assert!(!d.applied);
    let Sel::Edge { edge, .. } = edge_at(&s, 10.0, 20.0, 0.0) else { panic!() };
    assert_eq!(picked, Some(Pick::Edge { body: s.model.bodies[0].id, edge }));
    // the picks are back on the plain model; adding the pick previews both edges
    s.selection.push(Sel::Edge { body: s.model.bodies[0].id, edge });
    frame(&mut s, &mut d);
    assert!(d.applied);
    assert_eq!(edge_count(&s), 2);

    let b = &s.model.bodies[0];
    let face = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
    let hover = Pick::Face { body: b.id, face: face.id };
    let picked = d.unpreview_pick(&mut s, Some(hover));
    let Sel::Face { face, .. } = top(&s) else { panic!() };
    assert_eq!(picked, Some(Pick::Face { body: s.model.bodies[0].id, face }));
  }

  #[test]
  fn fillet_sets_with_their_own_radii() {
    let mut s = block();
    let mut d = Dialog::blank(Kind::Fillet);
    s.selection = vec![edge_at(&s, 10.0, 0.0, 20.0)];
    d.radius = "1 mm".into();
    frame(&mut s, &mut d);
    // + : a second set, picked and given its own radius
    d.withdraw(&mut s);
    d.store_fillet_set();
    d.solid.rows.push(super::super::solid::FilletRow { radius: "3 mm".into(), ..Default::default() });
    d.switch_fillet_set(&mut s, 1);
    assert!(s.selection.is_empty(), "the new set starts empty");
    s.selection = vec![edge_at(&s, 10.0, 20.0, 20.0)];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let cut = |r: f64| (1.0 - std::f64::consts::PI / 4.0) * r * r * 20.0;
    assert!((8000.0 - volume(&s) - cut(1.0) - cut(3.0)).abs() < 1e-3, "{}", 8000.0 - volume(&s));
    // back to the first set: its edge is the selection again
    d.switch_fillet_set(&mut s, 0);
    assert_eq!(s.selection, vec![edge_at(&s, 10.0, 0.0, 20.0)]);
    frame(&mut s, &mut d);
    assert!(d.applied);

    // edit from the timeline: each set's radius
    let id = s.design.features.last().unwrap().id;
    let app_free_edit = |s: &Session| {
      let f = s.design.feature(id).unwrap();
      let FeatureKind::Fillet { sets, radius, .. } = &f.kind else { panic!() };
      (radius.clone(), sets.iter().map(|x| x.radius.clone()).collect::<Vec<_>>())
    };
    assert_eq!(app_free_edit(&s), ("1 mm".to_string(), vec!["3 mm".to_string()]));
    let mut e = Dialog::blank(Kind::Fillet);
    e.editing = Some(id);
    let FeatureKind::Fillet { sets, radius, edges, .. } = s.design.feature(id).unwrap().kind.clone() else { panic!() };
    e.radius = radius;
    e.edges = vec![Value::Null; edges.len()];
    e.load_fillet_sets(&sets);
    e.solid.rows[1].radius = "2 mm".into();
    e.preview(&mut s);
    assert!(e.applied && e.error.is_none(), "{:?}", e.error);
    assert_eq!(app_free_edit(&s).1, vec!["2 mm".to_string()]);
    assert!((8000.0 - volume(&s) - cut(1.0) - cut(2.0)).abs() < 1e-3);
  }

  #[test]
  fn tangent_chain_is_on_by_default() {
    let mut s = block();
    run(&mut s, "fillet", json!({"edges": [{"point": [20, 20, 10]}], "radius": 5}));
    let mut d = Dialog::blank(Kind::Fillet);
    d.radius = "1 mm".into();
    s.selection = vec![edge_at(&s, 7.5, 20.0, 20.0)];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert_eq!(edge_count(&s), 3, "line, arc, line");
    d.withdraw(&mut s);
    d.solid.no_chain = true;
    frame(&mut s, &mut d);
    assert_eq!(edge_count(&s), 1);
  }
}
