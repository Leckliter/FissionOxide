//! Review Warning: the failed and warned features of the regenerated design, as Fusion's Error
//! window lists them (Fillet1 > Compute Failed > what went wrong and what to try).
//!
//! The messages are the engine's own feature errors and warnings, led by Fusion's explanation
//! for the kind of feature when it has one.

use fox_model::{FeatureId, FeatureKind};

use crate::Session;

/// How bad a reviewed feature is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
  /// The feature failed to compute (the timeline frames it red).
  Error,
  /// It computed with a warning (framed yellow).
  Warning,
}

impl Severity {
  /// The tree's middle row, as Fusion words it.
  pub fn heading(self) -> &'static str {
    match self {
      Severity::Error => "Compute Failed",
      Severity::Warning => "Compute Warning",
    }
  }
}

/// One feature's entry in the Error window.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
  pub feature: FeatureId,
  /// The feature's timeline name ("Fillet1").
  pub name: String,
  pub severity: Severity,
  /// What went wrong, in Fusion's words where it has some, then the engine's detail.
  pub message: String,
}

/// Every failed or warned feature, in timeline order. `errors_only` leaves the warnings out.
pub fn review(s: &Session, errors_only: bool) -> Vec<Review> {
  let mut out = Vec::new();
  for st in &s.model.status {
    let id = FeatureId(st.id);
    let kind = s.design.feature(id).map(|f| &f.kind);
    let (severity, detail) = match (&st.error, &st.warning) {
      (Some(e), _) => (Severity::Error, e),
      (None, Some(w)) if !errors_only => (Severity::Warning, w),
      _ => continue,
    };
    out.push(Review { feature: id, name: st.name.clone(), severity, message: message(kind, severity, detail) });
  }
  out
}

/// The review of one feature, if it failed or warned.
pub fn review_feature(s: &Session, id: FeatureId) -> Option<Review> {
  review(s, false).into_iter().find(|r| r.feature == id)
}

/// Fusion's explanation for a failed feature of this kind, followed by the engine's detail.
pub fn message(kind: Option<&FeatureKind>, severity: Severity, detail: &str) -> String {
  let detail = detail.trim();
  let lead = match (severity, kind) {
    (
      Severity::Error,
      Some(FeatureKind::Fillet { .. } | FeatureKind::FullRoundFillet { .. } | FeatureKind::Chamfer { .. }),
    ) => Some(
      "The fillet/chamfer could not be created where edges meet at a corner, or the size is too large \
       for the faces next to the edges. Try adjusting the size of the fillet/chamfer or changing the \
       selected edges.",
    ),
    (Severity::Error, Some(FeatureKind::Shell { .. })) => {
      Some("The shell could not be created. Try a smaller thickness, or remove fewer faces.")
    }
    (Severity::Error, Some(FeatureKind::Sketch { .. })) => {
      Some("The sketch could not be computed. Edit the sketch and check its constraints and dimensions.")
    }
    (Severity::Error, _) => Some("The feature could not be computed. Edit the feature and check its inputs."),
    (Severity::Warning, _) => None,
  };
  match lead {
    Some(l) if detail.is_empty() => l.to_string(),
    Some(l) => format!("{l}\n{}", capitalise(detail)),
    None => capitalise(detail),
  }
}

fn capitalise(s: &str) -> String {
  let mut c = s.chars();
  match c.next() {
    Some(f) => f.to_uppercase().chain(c).collect(),
    None => String::new(),
  }
}

/// The Error window's text for the copy button: each feature, its heading and message, indented
/// as the tree shows them.
pub fn report_text(items: &[Review]) -> String {
  let mut out = String::new();
  for r in items {
    out.push_str(&r.name);
    out.push('\n');
    out.push_str("  ");
    out.push_str(r.severity.heading());
    out.push('\n');
    for line in r.message.lines() {
      out.push_str("    ");
      out.push_str(line);
      out.push('\n');
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  /// The batch's scene: a 10 mm box with every edge filleted by filletR, raised from 2 to 8 mm.
  #[test]
  fn a_fillet_too_big_for_its_box_is_reviewed_as_fusion_words_it() {
    let mut s = Session::default();
    s.execute("param.add", &json!({"name": "filletR", "expr": "2 mm"})).unwrap();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let body = s.model.bodies[0].id.0;
    let edges: Vec<_> = s.model.bodies[0].edges().iter().map(|e| json!({"body": body, "edge": e.id})).collect();
    s.execute("fillet", &json!({"edges": edges, "radius": "filletR"})).unwrap();
    assert!(review(&s, false).is_empty(), "2 mm fits: {:?}", review(&s, false));
    s.execute("param.set", &json!({"name": "filletR", "expr": "8 mm"})).unwrap();
    let r = review(&s, false);
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].name, "Fillet1");
    assert_eq!(r[0].severity, Severity::Error);
    assert_eq!(r[0].severity.heading(), "Compute Failed");
    assert!(r[0].message.starts_with("The fillet/chamfer could not be created"), "{}", r[0].message);
    // the engine's own detail follows Fusion's explanation
    let engine = s.model.status(r[0].feature).and_then(|st| st.error.clone()).unwrap();
    assert!(r[0].message.to_lowercase().contains(&engine.trim().to_lowercase()), "{} / {engine}", r[0].message);
    assert_eq!(review_feature(&s, r[0].feature), Some(r[0].clone()));
    let text = report_text(&r);
    assert!(text.starts_with("Fillet1\n  Compute Failed\n    The fillet/chamfer"), "{text}");
  }

  /// The box filleted by filletR at `r`, in a design with the given behaviours.
  fn filleted_box(behaviours: fox_model::Behaviours, r: &str) -> Session {
    let mut s = Session::default();
    s.design.behaviours = behaviours;
    s.execute("param.add", &json!({"name": "filletR", "expr": "2 mm"})).unwrap();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let body = s.model.bodies[0].id.0;
    let edges: Vec<_> = s.model.bodies[0].edges().iter().map(|e| json!({"body": body, "edge": e.id})).collect();
    s.execute("fillet", &json!({"edges": edges, "radius": "filletR"})).unwrap();
    s.execute("param.set", &json!({"name": "filletR", "expr": r})).unwrap();
    s
  }

  #[test]
  fn fillets_that_fit_still_round_in_a_new_design() {
    for r in ["2 mm", "4.9 mm"] {
      let s = filleted_box(fox_model::Behaviours::current(), r);
      assert!(s.model.errors().is_empty(), "{r}: {:?}", s.model.errors());
      let v: f64 = s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum();
      assert!(v > 0.0 && v < 1000.0, "{r}: {v}");
    }
  }

  #[test]
  fn an_old_design_keeps_its_oversized_fillet_body() {
    // no FILLET_VALID_RESULT: the body OCCT made stays, with no error, as before
    let s = filleted_box(fox_model::Behaviours::legacy(), "8 mm");
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    assert_eq!(s.model.bodies.len(), 1);
  }

  #[test]
  fn warnings_are_reviewed_unless_only_errors_are_asked_for() {
    assert_eq!(message(None, Severity::Warning, "profile lost"), "Profile lost");
    assert_eq!(Severity::Warning.heading(), "Compute Warning");
    let m = message(None, Severity::Error, "no such face");
    assert!(m.starts_with("The feature could not be computed") && m.ends_with("No such face"), "{m}");
  }
}
