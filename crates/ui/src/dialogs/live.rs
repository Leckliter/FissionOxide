//! Live preview, on by default as in Fusion: a model-picking dialog (Fillet, Chamfer, Shell, …)
//! previews as soon as it has a selection and again whenever a pick or value changes. A viewport
//! click withdraws the preview and goes on to pick on the un-previewed model, and bodies a preview
//! adds stay out of the browser until OK.

use fox_engine::Session;
use fox_engine::model::BodyId;
use fox_engine::scene::Pick;
use serde_json::{Value, json};

use super::Dialog;

#[derive(Debug, Clone)]
pub(super) struct Live {
  /// The Preview checkbox.
  pub(super) on: bool,
  /// The command the last preview tried: a failed one isn't retried until something changes.
  tried: Option<Value>,
  /// The bodies before the preview.
  before: Vec<BodyId>,
}

impl Default for Live {
  fn default() -> Live {
    Live { on: true, tried: None, before: Vec::new() }
  }
}

impl Dialog {
  /// Asks for a preview when live preview is on and the command changed since the last one.
  pub(super) fn live_check(&mut self) {
    if !self.live.on || !self.picks() || self.editing.is_some() || self.dirty || !self.ready() {
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
  }

  /// The Preview checkbox of a model-picking dialog.
  pub(super) fn live_checkbox(&mut self, ui: &mut egui::Ui, s: &mut Session) {
    if ui.checkbox(&mut self.live.on, "Preview").changed() {
      if self.live.on {
        self.live.tried = None;
      } else {
        self.withdraw(s);
      }
    }
  }

  /// Bodies the showing preview added (the browser leaves them out until OK).
  pub fn preview_bodies(&self, s: &Session) -> Vec<BodyId> {
    if !self.applied || self.editing.is_some() {
      return Vec::new();
    }
    s.model.bodies.iter().map(|b| b.id).filter(|b| !self.live.before.contains(b)).collect()
  }

  /// Withdraws a model-picking dialog's preview for a viewport click, and finds what was under
  /// the cursor (`hover`, picked on the preview) on the un-previewed model: the same edge or
  /// face, when the preview didn't change it. None when there is nothing to pick there.
  pub fn unpreview_pick(&mut self, s: &mut Session, hover: Option<Pick>) -> Option<Pick> {
    enum Was {
      Edge(BodyId, fox_engine::kernel::EdgeInfo),
      Face(BodyId, fox_engine::kernel::FaceInfo),
    }
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
    match was? {
      Was::Edge(body, e) => {
        let b = s.model.body(body)?;
        let tol = 1e-6 * (1.0 + b.diagonal());
        // a preview may shorten the edge (a fillet at its end): the same curve, nearest middle
        let dir = (e.end - e.start).normalize_or_zero();
        let same = |x: &&fox_engine::kernel::EdgeInfo| {
          use fox_engine::kernel::CurveKind;
          x.kind == e.kind
            && match e.kind {
              CurveKind::Line => {
                (x.end - x.start).normalize_or_zero().cross(dir).length() < 1e-6
                  && (x.mid - e.mid).cross(dir).length() < tol
              }
              CurveKind::Circle => {
                (x.center - e.center).length() < tol
                  && (x.radius - e.radius).abs() < tol
                  && x.axis.cross(e.axis).length() < 1e-6
              }
              _ => (x.mid - e.mid).length() < tol,
            }
        };
        b.edges()
          .iter()
          .filter(same)
          .min_by(|x, y| (x.mid - e.mid).length().total_cmp(&(y.mid - e.mid).length()))
          .map(|x| Pick::Edge { body, edge: x.id })
      }
      Was::Face(body, f) => {
        let b = s.model.body(body)?;
        let tol = 1e-6 * (1.0 + b.diagonal());
        // a preview may trim the face (a fillet along its edge): the same surface, nearest centre
        let same = |x: &&fox_engine::kernel::FaceInfo| {
          x.kind == f.kind
            && x.normal.dot(f.normal) > 1.0 - 1e-9
            && if f.kind == fox_engine::kernel::SurfaceKind::Plane {
              (x.origin - f.origin).dot(f.normal).abs() < tol
            } else {
              (x.origin - f.origin).cross(f.normal).length() < tol && (x.radius - f.radius).abs() < tol
            }
        };
        b.faces()
          .iter()
          .filter(same)
          .min_by(|x, y| (x.center - f.center).length().total_cmp(&(y.center - f.center).length()))
          .map(|x| Pick::Face { body, face: x.id })
      }
    }
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
