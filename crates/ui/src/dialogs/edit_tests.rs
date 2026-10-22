//! Edit Feature as Fusion does it: the design rolls back to the feature while its dialog is open,
//! the dialog is the creation dialog filled in (live picks, the same handles), and a value that
//! makes a feature fail greys OK and raises the error toast, as does an edit that breaks a later
//! feature.

use egui::{Pos2, RawInput, Rect, vec2};
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::FeatureId;
use fox_engine::{Sel, Session};
use serde_json::json;

use super::*;
use crate::panels::{error_title, failed_features};

/// The start of Fusion's message for a fillet too big for its edges.
const TOO_BIG: &str = "The fillet/chamfer could not be created at the requested size.";

/// One frame of the whole app (a dialog's OK is handled in its frame).
fn frame(ctx: &egui::Context, app: &mut App, time: &mut f64) {
  *time += 1.0 / 60.0;
  let input = RawInput {
    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
    time: Some(*time),
    ..Default::default()
  };
  ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
}

/// OK on the open dialog, as the marking menu asks for it.
fn ok(app: &mut App) {
  let ctx = egui::Context::default();
  let mut time = 0.0;
  frame(&ctx, app, &mut time);
  app.dialog.as_mut().expect("a dialog").ok_requested = true;
  frame(&ctx, app, &mut time);
}

/// The top front edge of the box (its midpoint at y = -10, z = `height`).
fn top_front_edge(s: &Session, height: f64) -> Value {
  let b = &s.model.bodies[0];
  let e = b.edges().iter().find(|e| (e.mid - DVec3::new(0.0, -10.0, height)).length() < 1e-6).unwrap();
  json!({"body": b.id.0, "edge": e.id})
}

/// A 20 mm cube (Box1) with a fillet of `radius` on its top front edge (Fillet1).
fn box_and_fillet(radius: &str) -> (App, FeatureId, FeatureId) {
  let mut s = Session::default();
  s.execute("box", &json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  let edge = top_front_edge(&s, 20.0);
  s.execute("fillet", &json!({"edges": [edge], "radius": radius})).unwrap();
  let ids: Vec<FeatureId> = s.design.features.iter().map(|f| f.id).collect();
  (App::with_session(s), ids[0], ids[1])
}

fn top(app: &App) -> f64 {
  app.s.model.bodies[0].shape.mass().unwrap().bbox_max.z
}

fn rolled_back(app: &App, id: FeatureId) -> bool {
  app.s.model.status(id).is_some_and(|st| st.rolled_back)
}

/// Edit Feature rolls the timeline back to the item (the marker just after it, later items
/// rolled back) until OK, which rolls forward and leaves one undo step; Cancel restores the
/// design as it was.
#[test]
fn editing_rolls_the_design_back_until_ok_or_cancel() {
  let (mut app, box1, fillet) = box_and_fillet("2 mm");
  let before = app.s.design.clone();
  crate::panels::edit_feature(&mut app, box1);
  assert_eq!(app.s.design.marker, Some(1), "the marker sits just after Box1");
  assert!(rolled_back(&app, fillet) && !rolled_back(&app, box1));
  let d = app.dialog.as_mut().unwrap();
  assert!(d.applied && d.editing == Some(box1));
  assert_eq!(d.plane_label, "XY plane", "the Placement row names the box's plane");
  d.c = "30 mm".into();
  d.dirty = true;
  ok(&mut app);
  assert!(app.dialog.is_none(), "OK closed the dialog");
  assert_eq!(app.s.design.marker, None, "rolled forward to the end");
  assert!(!rolled_back(&app, fillet));
  assert!((top(&app) - 30.0).abs() < 1e-6);
  // one undo step takes the whole edit back
  assert!(app.s.undo());
  assert_eq!(app.s.design, before);

  crate::panels::edit_feature(&mut app, box1);
  let d = app.dialog.as_mut().unwrap();
  d.c = "40 mm".into();
  d.preview(&mut app.s);
  assert!((top(&app) - 40.0).abs() < 1e-6, "the edit previews on the rolled-back design");
  app.cancel_dialog();
  assert_eq!(app.s.design, before, "Cancel restores the design");
  assert!(!app.s.in_transaction());
}

/// The edited fillet's dialog is the creation dialog filled in: its edge is picked again on the
/// rolled-back body (selected, a live chip), and a new pick changes the feature's edge.
#[test]
fn an_edited_fillets_picks_are_live() {
  let (mut app, _, fillet) = box_and_fillet("2 mm");
  crate::panels::edit_feature(&mut app, fillet);
  // just after the last feature is the end
  assert_eq!(app.s.design.marker, None);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks && !d.picks_fixed());
  // found on the body before the fillet, the same edge as was picked
  d.withdraw(&mut app.s);
  assert_eq!(d.edges, vec![top_front_edge(&app.s, 20.0)]);
  assert_eq!(app.s.selection, d.picked_selection());
  assert_eq!(d.fillet_sets()[0].label(), "1 Edge");
  // pick the top back edge instead
  let b = &app.s.model.bodies[0];
  let back = b.edges().iter().find(|e| (e.mid - DVec3::new(0.0, 10.0, 20.0)).length() < 1e-6).unwrap().id;
  app.s.selection = vec![Sel::Edge { body: b.id, edge: back }];
  d.sync_selection(&app.s);
  assert_eq!(d.edges, vec![json!({"body": b.id.0, "edge": back})]);
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  assert_eq!(app.s.design.marker, None, "the preview puts the marker back after the fillet");
  let FeatureKind::Fillet { edges, .. } = &app.s.design.feature(fillet).unwrap().kind else { panic!() };
  assert!((DVec3::from(edges[0].point) - DVec3::new(0.0, 10.0, 20.0)).length() < 1e-6, "{edges:?}");
  app.dialog = Some(d);
  ok(&mut app);
  assert!(app.dialog.is_none() && app.s.design.marker.is_none());
}

/// Edit Extrude: the profiles are picked again (selected in the sketch) and the taper handle
/// shows as during creation.
#[test]
fn an_edited_extrude_has_its_profiles_and_taper_handle() {
  let mut s = Session::default();
  s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
  s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [10, 10]})).unwrap();
  let sk = s.edit_sketch.unwrap();
  s.execute("sketch.finish", &json!({})).unwrap();
  s.execute("extrude", &json!({"sketch": sk.0, "profiles": [[5, 5]], "distance": "10 mm", "taper": "-3 deg"})).unwrap();
  let mut app = App::with_session(s);
  let id = app.s.design.features.last().unwrap().id;
  crate::panels::edit_feature(&mut app, id);
  let d = app.dialog.as_ref().unwrap();
  assert!(d.repicks && d.ok_enabled());
  assert_eq!(d.profiles.len(), 1);
  assert!(matches!(app.s.selection.as_slice(), [Sel::Profile { sketch, index: 0 }] if *sketch == sk));
  assert_eq!(d.handles(&app.s).len(), 1, "the taper handle");
  assert!(d.manipulator(&app.s).is_some(), "the distance arrow");
  app.cancel_dialog();
}

/// A value that makes the fillet fail greys OK (OK from the marking menu does nothing), the
/// error toast reads "1 error(s)" with Fusion's message, and a value that works clears it.
#[test]
fn a_failing_value_greys_ok_and_raises_the_toast() {
  let (mut app, _, fillet) = box_and_fillet("2 mm");
  crate::panels::edit_feature(&mut app, fillet);
  let d = app.dialog.as_mut().unwrap();
  d.radius = "50 mm".into();
  d.preview(&mut app.s);
  assert!(d.failed() && !d.ok_enabled(), "{:?}", d.error);
  let failed = failed_features(&app);
  assert_eq!(failed.len(), 1);
  assert_eq!(failed[0].0, fillet);
  assert!(failed[0].1.starts_with(TOO_BIG), "the message, not the name: {}", failed[0].1);
  assert_eq!(error_title(failed.len()), "1 error(s)");
  ok(&mut app);
  assert!(app.dialog.is_some(), "OK is greyed while the preview fails");
  assert_eq!(app.error_toast.failed, vec![fillet], "the toast and the badge are up");
  let d = app.dialog.as_mut().unwrap();
  d.radius = "1 mm".into();
  d.preview(&mut app.s);
  assert!(!d.failed() && d.ok_enabled());
  assert!(failed_features(&app).is_empty());
  ok(&mut app);
  assert!(app.dialog.is_none());
  assert!(app.error_toast.failed.is_empty(), "a value that works clears the badge");
}

/// An upstream edit that breaks a later feature raises the toast naming that feature (and the
/// badge stays while it fails); while the edit's dialog is open the feature is rolled back, so
/// nothing shows yet.
#[test]
fn an_edit_that_breaks_a_later_feature_raises_the_toast() {
  let (mut app, box1, fillet) = box_and_fillet("5 mm");
  crate::panels::edit_feature(&mut app, box1);
  let d = app.dialog.as_mut().unwrap();
  d.c = "1 mm".into();
  d.preview(&mut app.s);
  assert!(failed_features(&app).is_empty(), "Fillet1 is rolled back during the edit");
  ok(&mut app);
  assert!(app.dialog.is_none());
  let failed = failed_features(&app);
  assert_eq!(failed, vec![(fillet, "Fillet1".to_string())], "the feature's name");
  let mut toast = crate::panels::ErrorToast::default();
  crate::panels::note_failures(&mut toast, &failed, 5.0);
  assert_eq!(toast.since, Some(5.0), "a new failure brings the toast up");
  crate::panels::note_failures(&mut toast, &failed, 9.0);
  assert_eq!(toast.since, Some(5.0), "the same failure doesn't bring it up again");
  crate::panels::note_failures(&mut toast, &[], 10.0);
  assert!(toast.failed.is_empty());
}
