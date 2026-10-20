//! Sketch Fillet as in Fusion: no side dialog. Click a corner (or two lines or arcs) and the
//! fillet previews at once, with a heads-up radius box at the arc and a drag arrow. Enter or a
//! click accepts it and the tool stays active for the next corner; Esc withdraws the preview.
//!
//! The preview is the real `sketch.fillet` command applied to the session (one undo step, as a
//! dialog's preview is), so the arc carries its radius dimension and tangent constraints from the
//! start; changing the radius withdraws it and applies the new one.

use fox_engine::Session;
use fox_engine::model::FeatureId;
use fox_engine::scene::Pick;
use fox_engine::sketch::{CurveId, CurveKind, PointId, Sketch};
use serde_json::{Value, json};

/// The tooltip while no curve is picked (Fusion's wording).
pub const HINT_FIRST: &str = "Select first line or arc";
/// The tooltip once one curve is picked.
pub const HINT_SECOND: &str = "Select second line or arc";
/// The tooltip while a fillet previews.
pub const HINT_PREVIEW: &str = "Type a radius or drag the arrow · Enter or click to accept · Esc to cancel";
/// Fusion's starting radius.
pub const DEFAULT_RADIUS: &str = "5 mm";

/// A fillet applied to the session as a preview.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
  pub sketch: FeatureId,
  pub curves: [u32; 2],
  pub arc: u32,
}

/// The arc a preview made, in sketch coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcInfo {
  pub center: [f64; 2],
  /// The middle of the arc: where the radius box and the arrow sit.
  pub mid: [f64; 2],
  pub radius: f64,
  /// The corner the fillet rounds: where the tangents at the arc's ends meet.
  pub corner: [f64; 2],
}

/// The Fillet tool's state between clicks.
#[derive(Debug, Clone)]
pub struct FilletFlow {
  /// The first curve picked, waiting for the second.
  pub first: Option<u32>,
  pub preview: Option<Preview>,
  /// The radius as typed; kept from one corner to the next, as Fusion does.
  pub radius: String,
  /// Why the last fillet couldn't be made.
  pub error: Option<String>,
  /// The radius box was (re)opened: it takes the keyboard with its text selected.
  pub fresh: bool,
  /// While the arrow is dragged: how far the pointer grabbed it past the arc's middle.
  pub grab: Option<f64>,
}

impl Default for FilletFlow {
  fn default() -> FilletFlow {
    FilletFlow { first: None, preview: None, radius: DEFAULT_RADIUS.into(), error: None, fresh: false, grab: None }
  }
}

impl FilletFlow {
  /// A flow starting with Fusion's default radius in the design's unit.
  pub fn new(unit: fox_engine::model::LengthUnit) -> FilletFlow {
    FilletFlow { radius: unit.localize(DEFAULT_RADIUS), ..FilletFlow::default() }
  }

  pub fn is_idle(&self) -> bool {
    self.first.is_none() && self.preview.is_none()
  }

  /// What the tooltip asks for now.
  pub fn hint(&self) -> &'static str {
    match (&self.preview, self.first) {
      (Some(_), _) => HINT_PREVIEW,
      (None, Some(_)) => HINT_SECOND,
      (None, None) => HINT_FIRST,
    }
  }

  /// Drops a preview that is no longer in the design (undone, or another sketch is open): it
  /// stays as it is, there's nothing to withdraw.
  pub fn validate(&mut self, s: &Session) {
    let gone = self.preview.as_ref().is_some_and(|p| {
      s.edit_sketch != Some(p.sketch)
        || s.design.sketch(p.sketch).map_or(true, |sk| sk.curve(CurveId(p.arc)).is_none())
        || !s.can_undo()
    });
    if gone {
      self.preview = None;
    }
    if self.first.is_some_and(|c| {
      s.edit_sketch.and_then(|sk| s.design.sketch(sk).ok()).is_none_or(|sk| sk.curve(CurveId(c)).is_none())
    }) {
      self.first = None;
    }
  }

  /// A click in the sketch on `pick`. A showing preview is accepted first (the click is Fusion's
  /// accept); a corner point picks the two curves that meet there, a line or arc is the first or
  /// second curve. Returns true when the click was used.
  pub fn click(&mut self, s: &mut Session, sk: FeatureId, pick: Option<Pick>) -> bool {
    self.validate(s);
    let accepted = self.accept();
    self.error = None;
    match pick {
      Some(Pick::SketchPoint { sketch, point }) if sketch == sk => {
        let Some(pair) = s.design.sketch(sk).ok().and_then(|k| corner_curves(k, point)) else {
          return accepted;
        };
        self.first = None;
        self.apply(s, sk, pair);
        true
      }
      Some(Pick::SketchCurve { sketch, curve }) if sketch == sk => {
        let fillets = s
          .design
          .sketch(sk)
          .ok()
          .and_then(|k| k.curve(curve))
          .is_some_and(|c| matches!(c.kind, CurveKind::Line { .. } | CurveKind::Arc { .. }));
        if !fillets {
          self.error = Some("Fillet takes lines and arcs".into());
          return accepted;
        }
        match self.first {
          Some(f) if f != curve.0 => {
            self.first = None;
            self.apply(s, sk, [f, curve.0]);
          }
          Some(_) => {}
          None => self.first = Some(curve.0),
        }
        true
      }
      _ => accepted,
    }
  }

  /// Applies the fillet of `curves` at the current radius as the preview.
  fn apply(&mut self, s: &mut Session, sk: FeatureId, curves: [u32; 2]) -> bool {
    let radius = s.design.units.typed(&self.radius);
    let p = json!({"sketch": sk.0, "curves": curves, "radius": radius});
    match s.execute("sketch.fillet", &p) {
      Ok(v) => match v["curves"].get(0).and_then(Value::as_u64) {
        Some(arc) => {
          self.preview = Some(Preview { sketch: sk, curves, arc: arc as u32 });
          self.fresh = true;
          true
        }
        None => false,
      },
      Err(e) => {
        self.error = Some(e.0);
        false
      }
    }
  }

  /// A new radius for the preview (typed or dragged): the preview is withdrawn and made again.
  /// A radius that doesn't fit leaves the last good one showing.
  pub fn set_radius(&mut self, s: &mut Session, text: String) {
    if text == self.radius {
      return;
    }
    self.validate(s);
    let old = std::mem::replace(&mut self.radius, text);
    let Some(p) = self.preview.take() else { return };
    s.undo();
    s.drop_redo();
    if !self.apply(s, p.sketch, p.curves) {
      let err = self.error.take();
      self.radius = old;
      self.apply(s, p.sketch, p.curves);
      self.error = err;
    }
    self.fresh = false;
  }

  /// Keeps the preview: the fillet is done and the tool waits for the next corner.
  pub fn accept(&mut self) -> bool {
    self.fresh = false;
    self.preview.take().is_some()
  }

  /// Esc: withdraws the preview (and forgets a half-made pick).
  pub fn cancel(&mut self, s: &mut Session) {
    self.validate(s);
    if self.preview.take().is_some() {
      s.undo();
      s.drop_redo();
    }
    self.first = None;
    self.error = None;
    self.fresh = false;
  }

  /// The previewed arc's geometry.
  pub fn arc(&self, s: &Session) -> Option<ArcInfo> {
    let p = self.preview.as_ref()?;
    let sk = s.design.sketch(p.sketch).ok()?;
    arc_info(sk, CurveId(p.arc))
  }
}

/// The two lines or arcs that meet at `point`, a corner Fillet can round.
pub fn corner_curves(sk: &Sketch, point: PointId) -> Option<[u32; 2]> {
  let at = sk.pos(point)?;
  let near = |q: PointId| q == point || sk.pos(q).is_some_and(|p| (p[0] - at[0]).hypot(p[1] - at[1]) < 1e-6);
  let found: Vec<u32> = sk
    .curves
    .iter()
    .filter(|c| !c.construction)
    .filter(|c| match c.kind {
      CurveKind::Line { a, b } => near(a) || near(b),
      CurveKind::Arc { start, end, .. } => near(start) || near(end),
      _ => false,
    })
    .map(|c| c.id.0)
    .collect();
  match found.as_slice() {
    [a, b] => Some([*a, *b]),
    _ => None,
  }
}

/// An arc's centre, middle, radius and the corner its end tangents make.
pub fn arc_info(sk: &Sketch, arc: CurveId) -> Option<ArcInfo> {
  let CurveKind::Arc { center, start, end } = sk.curve(arc)?.kind else { return None };
  let (c, a, b) = (sk.pos(center)?, sk.pos(start)?, sk.pos(end)?);
  let radius = (a[0] - c[0]).hypot(a[1] - c[1]);
  let (e1, e2) = ((a[1] - c[1]).atan2(a[0] - c[0]), (b[1] - c[1]).atan2(b[0] - c[0]));
  let sweep = (e2 - e1).rem_euclid(std::f64::consts::TAU);
  let m = e1 + sweep / 2.0;
  let u = [m.cos(), m.sin()];
  let mid = [c[0] + u[0] * radius, c[1] + u[1] * radius];
  // the tangents at the ends meet on the bisector, r / cos(sweep / 2) from the centre
  let half = (sweep / 2.0).cos().max(1e-3);
  let corner = [c[0] + u[0] * radius / half, c[1] + u[1] * radius / half];
  Some(ArcInfo { center: c, mid, radius, corner })
}

/// The radius for a drag arrow at `along` from the corner (towards the arc's middle and on), as
/// a fillet grows away from its corner: the arc's middle sits `r · (1 / cos(sweep/2) − 1)` from it.
pub fn radius_for(info: &ArcInfo, along: f64) -> Option<f64> {
  let d = (info.mid[0] - info.corner[0]).hypot(info.mid[1] - info.corner[1]);
  let k = d / info.radius.max(1e-9);
  (k > 1e-6 && along > 0.0).then(|| along / k)
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::sketch::ConstraintKind;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    let v = s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    assert!(v.get("error").is_none_or(Value::is_null), "{cmd}: {v}");
    v
  }

  /// A 40×20 rectangle in a sketch being edited: the sketch and its four lines.
  fn rect() -> (Session, FeatureId, Vec<u32>) {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
    let lines = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    let sk = s.edit_sketch.unwrap();
    (s, sk, lines)
  }

  /// The point at a corner of the rectangle.
  fn corner_at(s: &Session, sk: FeatureId, at: [f64; 2]) -> PointId {
    let k = s.design.sketch(sk).unwrap();
    k.curves
      .iter()
      .flat_map(|c| k.curve_points(c.id))
      .find(|p| k.pos(*p).is_some_and(|q| (q[0] - at[0]).hypot(q[1] - at[1]) < 1e-9))
      .unwrap()
  }

  fn arcs(s: &Session, sk: FeatureId) -> usize {
    s.design.sketch(sk).unwrap().curves.iter().filter(|c| matches!(c.kind, CurveKind::Arc { .. })).count()
  }

  #[test]
  fn a_corner_click_previews_at_once_and_a_click_accepts_with_the_tool_still_active() {
    let (mut s, sk, _) = rect();
    let mut f = FilletFlow::default();
    assert_eq!(f.hint(), "Select first line or arc");
    assert_eq!(f.radius, "5 mm", "Fusion's default");

    // click the corner at the origin: the fillet previews straight away
    let p = corner_at(&s, sk, [0.0, 0.0]);
    assert!(f.click(&mut s, sk, Some(Pick::SketchPoint { sketch: sk, point: p })));
    let arc = f.preview.as_ref().expect("a preview").arc;
    assert_eq!(arcs(&s, sk), 1);
    let info = f.arc(&s).unwrap();
    assert!((info.radius - 5.0).abs() < 1e-6, "{info:?}");
    assert!((info.center[0] - 5.0).abs() < 1e-6 && (info.center[1] - 5.0).abs() < 1e-6, "{info:?}");
    assert!(info.corner[0].abs() < 1e-6 && info.corner[1].abs() < 1e-6, "{info:?}");
    assert!(f.fresh, "the radius box takes the keyboard");
    assert_eq!(f.hint(), HINT_PREVIEW);

    // as in Fusion, the fillet carries a radius dimension and two tangent constraints
    let k = s.design.sketch(sk).unwrap();
    let on_arc = |c: &ConstraintKind| match c {
      ConstraintKind::Tangent { a, b } => a.0 == arc || b.0 == arc,
      ConstraintKind::Radius { curve, .. } => curve.0 == arc,
      _ => false,
    };
    let kinds: Vec<_> = k.constraints.iter().map(|c| &c.kind).filter(|c| on_arc(c)).collect();
    assert_eq!(kinds.iter().filter(|c| matches!(c, ConstraintKind::Tangent { .. })).count(), 2, "{kinds:?}");
    assert_eq!(kinds.iter().filter(|c| matches!(c, ConstraintKind::Radius { .. })).count(), 1, "{kinds:?}");

    // typing a radius re-previews in place: still one fillet, one undo step
    f.set_radius(&mut s, "3 mm".into());
    assert_eq!(arcs(&s, sk), 1);
    assert!((f.arc(&s).unwrap().radius - 3.0).abs() < 1e-6);
    // a radius that doesn't fit keeps the last good one
    f.set_radius(&mut s, "50 mm".into());
    assert!(f.error.is_some());
    assert_eq!(f.radius, "3 mm");
    assert!((f.arc(&s).unwrap().radius - 3.0).abs() < 1e-6);

    // a click on empty canvas accepts it; the tool stays ready for the next corner
    assert!(f.click(&mut s, sk, None));
    assert!(f.preview.is_none() && f.first.is_none());
    assert_eq!(arcs(&s, sk), 1);
    assert_eq!(f.hint(), "Select first line or arc");

    // the next corner, picked as two lines (the radius carries over)
    let q = corner_at(&s, sk, [40.0, 20.0]);
    let pair = corner_curves(s.design.sketch(sk).unwrap(), q).unwrap();
    assert!(f.click(&mut s, sk, Some(Pick::SketchCurve { sketch: sk, curve: CurveId(pair[0]) })));
    assert_eq!(f.hint(), "Select second line or arc");
    assert_eq!(arcs(&s, sk), 1, "nothing yet");
    f.click(&mut s, sk, Some(Pick::SketchCurve { sketch: sk, curve: CurveId(pair[1]) }));
    assert_eq!(arcs(&s, sk), 2);
    assert!((f.arc(&s).unwrap().radius - 3.0).abs() < 1e-6);
    // Enter accepts too
    assert!(f.accept());
    assert_eq!(arcs(&s, sk), 2);

    // Esc withdraws a preview and leaves the earlier fillets alone
    let r = corner_at(&s, sk, [40.0, 0.0]);
    f.click(&mut s, sk, Some(Pick::SketchPoint { sketch: sk, point: r }));
    assert_eq!(arcs(&s, sk), 3);
    f.cancel(&mut s);
    assert_eq!(arcs(&s, sk), 2);
    assert!(f.is_idle());
  }

  #[test]
  fn an_undone_preview_is_forgotten_not_withdrawn_twice() {
    let (mut s, sk, _) = rect();
    let mut f = FilletFlow::default();
    let p = corner_at(&s, sk, [0.0, 0.0]);
    f.click(&mut s, sk, Some(Pick::SketchPoint { sketch: sk, point: p }));
    assert_eq!(arcs(&s, sk), 1);
    s.undo();
    f.cancel(&mut s);
    assert_eq!(arcs(&s, sk), 0);
    assert_eq!(s.design.sketch(sk).unwrap().curves.len(), 4, "the rectangle is untouched");
  }

  #[test]
  fn dragging_the_arrow_away_from_the_corner_grows_the_radius() {
    let (mut s, sk, _) = rect();
    let mut f = FilletFlow::default();
    let p = corner_at(&s, sk, [0.0, 0.0]);
    f.click(&mut s, sk, Some(Pick::SketchPoint { sketch: sk, point: p }));
    let info = f.arc(&s).unwrap();
    let d = (info.mid[0] - info.corner[0]).hypot(info.mid[1] - info.corner[1]);
    assert!((radius_for(&info, d).unwrap() - 5.0).abs() < 1e-9);
    assert!((radius_for(&info, 2.0 * d).unwrap() - 10.0).abs() < 1e-9);
    assert_eq!(radius_for(&info, -1.0), None);
  }
}
