//! The analyses kept with OK (Fusion's INSPECT analyses and Center of Mass): the browser's
//! Analysis folder lists them, each with an eye, and the canvas draws the visible ones. They
//! belong to the design open: a new or opened design starts without any.

use std::cell::Cell;

use fox_engine::kernel::glam::DVec3;
use serde_json::Value;

use crate::App;
use crate::dialogs::Kind;
use crate::icons::Icon;

/// One analysis in the Analysis folder.
#[derive(Debug, Clone)]
pub struct Kept {
  /// "Zebra1", "CenterOfMass1", ...
  pub name: String,
  /// The analysis: "zebra", "draft", "accessibility", "curvature", "environment", "isocurve",
  /// "min_radius", "comb" or "center_of_mass".
  pub kind: String,
  /// What it draws with: the render parameters (see `fox_engine::scene::view_options`), or
  /// Center of Mass's `{bodies}`.
  pub params: Value,
  /// Its eye.
  pub visible: bool,
  /// Center of Mass: the centre, at the design revision it was worked out for.
  center: Cell<Option<(u64, [f64; 3])>>,
}

/// The browser's Analysis folder's analyses (Section Analysis keeps its own rows,
/// [`crate::dialogs::KeptSection`]; the folder's eye, `App::analysis_hidden`, covers both).
#[derive(Debug, Clone, Default)]
pub struct Analyses {
  pub rows: Vec<Kept>,
}

/// The stem of a kept analysis's name, as Fusion names them ("CenterOfMass1").
pub fn base_name(kind: &str) -> &'static str {
  match kind {
    "zebra" => "Zebra",
    "draft" => "Draft",
    "accessibility" => "Accessibility",
    "environment" => "EnvironmentMap",
    "isocurve" => "Isocurve",
    "min_radius" => "MinimumRadius",
    "comb" => "CurvatureComb",
    "center_of_mass" => "CenterOfMass",
    "section" => "Section",
    _ => "CurvatureMap",
  }
}

/// A kept analysis's browser icon: its command's.
pub fn icon(kind: &str) -> Icon {
  match kind {
    "center_of_mass" => Icon::CenterOfMass,
    "section" => Icon::Section,
    "zebra" => Icon::Zebra,
    "draft" => Icon::DraftAnalysis,
    "accessibility" => Icon::Accessibility,
    "environment" => Icon::EnvMap,
    "isocurve" => Icon::Isocurve,
    "min_radius" => Icon::MinRadius,
    "comb" => Icon::CurvComb,
    _ => Icon::CurvMap,
  }
}

/// Keeps an analysis OK'd in its dialog: a new row at the end of the Analysis folder, named
/// after the kind with the next number. Returns the name.
pub fn keep(app: &mut App, kind: &str, params: Value) -> String {
  let stem = base_name(kind);
  let n = app.analyses.rows.iter().filter_map(|r| r.name.strip_prefix(stem)?.parse::<u32>().ok()).max().unwrap_or(0);
  let name = format!("{stem}{}", n + 1);
  app.analyses.rows.push(Kept {
    name: name.clone(),
    kind: kind.to_string(),
    params,
    visible: true,
    center: Cell::new(None),
  });
  app.analysis_hidden = false;
  name
}

/// Is a kept analysis drawn: its eye and the folder's are on.
pub fn drawn(app: &App, row: &Kept) -> bool {
  !app.analysis_hidden && row.visible
}

/// The analysis the canvas draws: the open analysis dialog's (once something is picked), else
/// the newest visible one in the Analysis folder.
pub fn render_params(app: &App) -> Option<Value> {
  if app.dialog.as_ref().is_some_and(|d| matches!(d.kind, Kind::Analysis(_))) {
    return app.analysis.clone();
  }
  app.analyses.rows.iter().rev().find(|r| drawn(app, r) && r.params.get("analysis").is_some()).map(|r| r.params.clone())
}

/// Works out the visible Center of Mass rows' centres again when the design changed. Called
/// once a frame, while the kernel is free.
pub fn refresh(app: &mut App) {
  let revision = app.s.revision;
  let stale: Vec<(usize, Vec<u32>)> = app
    .analyses
    .rows
    .iter()
    .enumerate()
    .filter(|(_, r)| r.kind == "center_of_mass" && r.visible)
    .filter(|(_, r)| r.center.get().is_none_or(|(rev, _)| rev != revision))
    .map(|(i, r)| {
      let bodies = r.params["bodies"].as_array().map(|a| a.iter().filter_map(|b| Some(b.as_u64()? as u32)).collect());
      (i, bodies.unwrap_or_default())
    })
    .collect();
  for (i, bodies) in stale {
    // bodies removed since: the centre of what is left, else none
    let live: Vec<u32> =
      bodies.into_iter().filter(|b| app.s.model.body(fox_engine::model::BodyId(*b)).is_some()).collect();
    let c = if live.is_empty() { None } else { crate::dialogs::center_of_mass(&mut app.s, &live) };
    if let Some(r) = app.analyses.rows.get(i) {
      r.center.set(c.map(|c| (revision, c)).or(Some((revision, [f64::NAN; 3]))));
    }
  }
}

/// The centre-of-mass glyphs the canvas draws: the open Center of Mass dialog's and the visible
/// kept ones'.
pub fn centers(app: &App) -> Vec<DVec3> {
  let mut out: Vec<DVec3> = app.dialog.as_ref().and_then(|d| d.marker()).into_iter().collect();
  for r in app.analyses.rows.iter().filter(|r| r.kind == "center_of_mass" && drawn(app, r)) {
    if let Some((_, c)) = r.center.get()
      && c.iter().all(|v| v.is_finite())
    {
      out.push(DVec3::from(c));
    }
  }
  out
}

/// Deletes a kept analysis (its row's Delete).
pub fn delete(app: &mut App, index: usize) {
  if index < app.analyses.rows.len() {
    app.analyses.rows.remove(index);
  }
}

/// A new or opened design starts with an empty Analysis folder.
pub fn clear(app: &mut App) {
  app.analyses = Analyses::default();
  app.analysis = None;
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dialogs::Dialog;
  use fox_engine::{Sel, Session};
  use serde_json::json;

  fn boxed() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10}));
    app.run("box", json!({"at": [40, 0], "width": 10, "depth": 10, "height": 10}));
    assert_eq!(app.s.model.bodies.len(), 2);
    app
  }

  fn frame(app: &mut App) -> bool {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
      ..Default::default()
    };
    let mut d = app.dialog.take().expect("a dialog");
    let mut keep = false;
    let mut out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
    out.textures_delta.clear();
    if keep {
      app.dialog = Some(d);
    } else {
      d.leave(app);
    }
    keep
  }

  fn ok(app: &mut App) -> bool {
    if let Some(d) = app.dialog.as_mut() {
      d.ok_requested = true;
    }
    frame(app)
  }

  #[test]
  fn analyses_wait_for_a_pick_and_ok_keeps_them_in_the_folder() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    for (kind, title) in [
      ("curvature", "CURVATURE MAP ANALYSIS"),
      ("zebra", "ZEBRA ANALYSIS"),
      ("environment", "ENVIRONMENT MAP ANALYSIS"),
      ("min_radius", "MINIMUM RADIUS ANALYSIS"),
    ] {
      app.dialog = Some(Dialog::open(&mut app, Kind::Analysis(kind)));
      assert!(frame(&mut app));
      assert!(render_params(&app).is_none(), "{title}: nothing is drawn until a pick");
      assert_eq!(
        app.dialog.as_ref().unwrap().prompt(false).as_deref(),
        Some(match kind {
          "min_radius" => "Select a body to analyze",
          _ => "Select bodies to analyze",
        })
      );
      // OK is greyed: asking for it keeps the dialog open
      assert!(ok(&mut app), "{title}: OK waits for a body");
      app.s.selection = vec![Sel::Face { body, face: 1 }];
      assert!(frame(&mut app));
      let p = render_params(&app).expect("drawn once a body is picked");
      assert_eq!(p["analysis"], kind);
      assert_eq!(p["bodies"], json!([body.0]), "only the picked body");
      assert!(!ok(&mut app), "{title}: OK closes");
    }
    let names: Vec<&str> = app.analyses.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["CurvatureMap1", "Zebra1", "EnvironmentMap1", "MinimumRadius1"]);
    // the newest visible one draws; its eye or the folder's hides it
    assert_eq!(render_params(&app).unwrap()["analysis"], "min_radius");
    app.analyses.rows[3].visible = false;
    assert_eq!(render_params(&app).unwrap()["analysis"], "environment");
    app.analysis_hidden = true;
    assert!(render_params(&app).is_none());
    app.analysis_hidden = false;
    // a second zebra is Zebra2
    app.dialog = Some(Dialog::open(&mut app, Kind::Analysis("zebra")));
    app.s.selection = vec![Sel::Body { body }];
    frame(&mut app);
    assert!(!ok(&mut app));
    assert_eq!(app.analyses.rows.last().unwrap().name, "Zebra2");
    // a new tab starts with no analyses; back on this one, they are as they were
    crate::documents::new_document(&mut app, fox_engine::model::DesignType::default(), None);
    assert!(app.analyses.rows.is_empty() && render_params(&app).is_none());
    crate::documents::switch_to(&mut app, 0);
    assert_eq!(app.analyses.rows.len(), 5);
    // a new design in place of this one starts with none
    app.new_design();
    assert!(app.analyses.rows.is_empty() && render_params(&app).is_none());
  }

  #[test]
  fn cancel_and_escape_leave_nothing_behind() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    app.dialog = Some(Dialog::open(&mut app, Kind::Analysis("zebra")));
    app.s.selection = vec![Sel::Body { body }];
    frame(&mut app);
    assert!(render_params(&app).is_some());
    app.cancel_dialog();
    assert!(app.analysis.is_none() && render_params(&app).is_none());
    assert!(app.analyses.rows.is_empty());
  }

  #[test]
  fn draft_and_accessibility_pick_a_body_then_a_direction() {
    let mut app = boxed();
    let body = app.s.model.bodies[1].id;
    for kind in ["draft", "accessibility"] {
      app.dialog = Some(Dialog::open(&mut app, Kind::Analysis(kind)));
      frame(&mut app);
      app.s.selection = vec![Sel::Face { body, face: 1 }];
      frame(&mut app);
      assert_eq!(app.dialog.as_ref().unwrap().prompt(false).as_deref(), Some("Select a direction"));
      assert!(render_params(&app).is_none());
      // the next pick, a planar face, is the direction (its normal)
      let top = app
        .s
        .model
        .body(body)
        .unwrap()
        .faces()
        .iter()
        .find(|f| f.normal.z > 0.9 && f.normal.length() > 0.5)
        .unwrap()
        .id;
      app.s.selection.push(Sel::Face { body, face: top });
      frame(&mut app);
      let p = render_params(&app).expect("drawn with a body and a direction");
      assert_eq!(p["bodies"], json!([body.0]), "the direction pick doesn't change the body");
      let pull: Vec<f64> = p["pull"].as_array().unwrap().iter().filter_map(Value::as_f64).collect();
      assert!((pull[2] - 1.0).abs() < 1e-6, "{pull:?}");
      if kind == "draft" {
        assert_eq!((p["lo"].as_f64(), p["hi"].as_f64(), p["tol"].as_f64()), (Some(-2.0), Some(2.0), Some(0.5)));
      }
      assert!(!ok(&mut app));
    }
    assert_eq!(app.analyses.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Draft1", "Accessibility1"]);
  }

  #[test]
  fn comb_and_isocurve_take_edges_and_faces() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    app.dialog = Some(Dialog::open(&mut app, Kind::Analysis("comb")));
    frame(&mut app);
    assert_eq!(app.dialog.as_ref().unwrap().prompt(false).as_deref(), Some("Select edges to analyze"));
    app.s.selection = vec![Sel::Edge { body, edge: 1 }];
    frame(&mut app);
    let p = render_params(&app).unwrap();
    assert_eq!(
      (p["edges"].clone(), p["density"].as_f64(), p["scale"].as_f64()),
      (json!([{"body": body.0, "edge": 1}]), Some(50.0), Some(10.0))
    );
    assert!(!ok(&mut app));
    app.dialog = Some(Dialog::open(&mut app, Kind::Analysis("isocurve")));
    app.s.selection = vec![Sel::Face { body, face: 2 }];
    frame(&mut app);
    let p = render_params(&app).unwrap();
    assert_eq!(
      (p["faces"].clone(), p["type"].as_str(), p["count"].as_f64()),
      (json!([{"body": body.0, "face": 2}]), Some("uv"), Some(5.0))
    );
    // the scene draws the isocurves over the face
    let opts = fox_engine::scene::view_options(&app.s, &p).unwrap();
    let (scene, _) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
    let iso = scene.lines.iter().filter(|l| l.color == [70, 70, 210, 255]).count();
    assert!(iso >= 10, "5 U and 5 V isocurves: {iso} segments");
    assert!(!ok(&mut app));
    assert_eq!(app.analyses.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["CurvatureComb1", "Isocurve1"]);
  }

  #[test]
  fn center_of_mass_needs_a_pick_and_keeps_a_glyph_row() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    app.dialog = Some(Dialog::open(&mut app, Kind::CenterOfMass));
    assert!(frame(&mut app));
    assert!(centers(&app).is_empty(), "nothing until a pick");
    assert_eq!(app.dialog.as_ref().unwrap().prompt(false).as_deref(), Some("Select bodies and/or components"));
    assert!(ok(&mut app), "OK waits for a pick");
    app.s.selection = vec![Sel::Body { body }];
    frame(&mut app);
    let c = centers(&app);
    assert_eq!(c.len(), 1);
    assert!((c[0].x).abs() < 1e-6 && (c[0].y).abs() < 1e-6, "the picked box's centre only: {c:?}");
    assert!(!ok(&mut app));
    assert_eq!(app.analyses.rows[0].name, "CenterOfMass1");
    assert_eq!(app.analyses.rows[0].params, json!({"bodies": [body.0]}));
    refresh(&mut app);
    assert_eq!(centers(&app).len(), 1, "the kept row's glyph");
    app.analyses.rows[0].visible = false;
    assert!(centers(&app).is_empty());
  }
}
