//! Create Flat Pattern and its environment, Export Flat Pattern, the browser's Flat Pattern row,
//! and Unfold with the REFOLD panel, driven as a user would.

use fox_engine::kernel::SurfaceKind;
use fox_engine::{Sel, Session};
use serde_json::json;

use super::{Dialog, Kind, sheet_flat};
use crate::App;
use crate::flat_ui::{self, FlatAct};
use crate::menus::{panel_commands, panel_labels};
use crate::panels::{Cmd, RowCmd, Tab};

/// A base flange with one edge flange (one bend).
fn sheet() -> App {
  let mut app = App::with_session(Session::default());
  app.run("sheet.rule", json!({"name": "Steel", "thickness": 1}));
  app.run("sketch.create", json!({"plane": "xy"}));
  app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
  app.finish_sketch();
  app.run("sheet.flange", json!({}));
  app.run("sheet.edge_flange", json!({"edges": [{"point": [40, 40, 1]}], "height": 15}));
  assert!(app.s.model.bodies[0].sheet.is_some());
  app
}

/// The base flange's bottom face, as `sheet.create_flat_pattern` takes it.
fn bottom_json(app: &App) -> serde_json::Value {
  let b = &app.s.model.bodies[0];
  let f = b.faces().iter().find(|f| f.kind == SurfaceKind::Plane && f.normal.z < -0.9).unwrap().id;
  json!({"body": b.id.0, "face": f})
}

/// The base flange's bottom face, selected.
fn bottom(app: &App) -> Sel {
  let v = bottom_json(app);
  Sel::Face {
    body: fox_engine::model::BodyId(v["body"].as_u64().unwrap() as u32),
    face: v["face"].as_u64().unwrap() as u32,
  }
}

/// Draws the dialog for one frame; returns whether it stays open.
fn show_once(app: &mut App, d: &mut Dialog) -> bool {
  let ctx = egui::Context::default();
  let raw = egui::RawInput {
    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
    ..Default::default()
  };
  let mut keep = false;
  let mut out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
  out.textures_delta.clear();
  keep
}

fn labels(app: &App, tab: Tab) -> Vec<&'static str> {
  panel_labels(app, tab).into_iter().map(|(l, _)| l).collect()
}

#[test]
fn create_flat_pattern_picks_a_stationary_face_then_enters_the_environment() {
  let mut app = sheet();
  app.start(Kind::FlatPattern);
  let mut d = app.dialog.take().expect("the dialog opens");
  assert_eq!(d.heading(), "CREATE FLAT PATTERN");
  assert!(!d.kind.reports(), "OK and Cancel, not an info panel");
  assert!(show_once(&mut app, &mut d));
  assert!(!d.ok_enabled());
  assert_eq!(d.prompt(false).as_deref(), Some("Select Stationary face to create Flat Pattern"));
  // a bend face is no stationary face
  let b = &app.s.model.bodies[0];
  let bend = b.faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
  app.s.selection = vec![Sel::Face { body: b.id, face: bend }];
  assert!(show_once(&mut app, &mut d));
  assert!(!d.ok_enabled());
  // the bottom face: OK lights, nothing made yet
  app.s.selection = vec![bottom(&app)];
  assert!(show_once(&mut app, &mut d));
  assert!(d.ok_enabled());
  assert!(app.s.design.flat_pattern.is_none());
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d), "OK closes it");
  assert!(app.s.design.flat_pattern.is_some());

  // the environment: its tabs' panels, ending in a green FINISH FLAT PATTERN
  assert!(app.flat_mode);
  assert_eq!(app.tab, Tab::Solid);
  assert_eq!(flat_ui::tab_label(Tab::Solid), "FLAT PATTERN SOLID");
  assert_eq!(flat_ui::tab_label(Tab::Surface), "FLAT PATTERN SURFACE");
  let want = ["CREATE", "MODIFY", "CONSTRUCT", "INSPECT", "INSERT", "SELECT", "EXPORT", "FINISH FLAT PATTERN"];
  assert_eq!(labels(&app, Tab::Solid), want);
  assert_eq!(labels(&app, Tab::Surface), want);
  assert_eq!(panel_commands(&app, Tab::Solid, "FINISH FLAT PATTERN"), ["Finish Flat Pattern"]);
  assert_eq!(panel_commands(&app, Tab::Solid, "EXPORT"), ["Export Flat Pattern"]);
  // the canvas draws the flat with both kinds of bend line
  let mut v = json!({});
  flat_ui::view_params(&app, &mut v);
  assert_eq!(v["flat"], json!({"center_lines": true, "extent_lines": true}));
  app.flat_lines[1] = false;
  flat_ui::view_params(&app, &mut v);
  assert_eq!(v["flat"]["extent_lines"], false);

  // Finish Flat Pattern: back on SHEET METAL, the flat pattern kept
  Cmd::Flat(FlatAct::Finish).run(&mut app);
  assert!(!app.flat_mode);
  assert_eq!(app.tab, Tab::SheetMetal);
  assert!(app.s.design.flat_pattern.is_some());
  let mut v = json!({});
  flat_ui::view_params(&app, &mut v);
  assert!(v.get("flat").is_none());
  assert!(!labels(&app, Tab::Solid).contains(&"FINISH FLAT PATTERN"));
}

#[test]
fn flat_pattern_row_activates_exports_and_deletes() {
  let mut app = sheet();
  app.run("sheet.create_flat_pattern", json!({"face": bottom_json(&app)}));
  let items: Vec<String> =
    crate::panels::browser_flat::row_menu_items().into_iter().map(|i| i.map(|i| i.label).unwrap_or_default()).collect();
  assert_eq!(items, ["Activate", "Export Flat Pattern", "", "Delete"]);
  crate::panels::browser_flat::run_row_cmd(&mut app, RowCmd::Activate);
  assert!(app.flat_mode);
  Cmd::Flat(FlatAct::Finish).run(&mut app);
  crate::panels::browser_flat::run_row_cmd(&mut app, RowCmd::ExportFlatPattern);
  let d = app.dialog.as_ref().expect("Export Flat Pattern opens");
  assert_eq!((d.kind, d.heading().as_str()), (Kind::ExportFlat, "EXPORT FLAT PATTERN"));
  app.cancel_dialog();
  crate::panels::browser_flat::run_row_cmd(&mut app, RowCmd::Delete);
  assert!(app.s.design.flat_pattern.is_none());
  // undo brings it back; deleting it from inside the environment leaves the environment
  app.s.undo();
  assert!(app.s.design.flat_pattern.is_some());
  flat_ui::activate(&mut app);
  app.run("sheet.delete_flat_pattern", json!({}));
  flat_ui::check(&mut app);
  assert!(!app.flat_mode);
}

#[test]
fn export_flat_pattern_writes_dxf_with_its_options_or_step() {
  let mut app = sheet();
  app.run("sheet.create_flat_pattern", json!({"face": bottom_json(&app)}));
  let mut d = Dialog::blank(Kind::ExportFlat);
  d.export_flat_begin(&app.s);
  // Fusion's defaults: DXF, the design's unit, splines kept, both bend line kinds
  let o = &d.flat;
  assert_eq!((o.file_type, o.units, o.polylines, o.center_lines, o.extent_lines), ("dxf", "mm", false, true, true));
  let dir = std::env::temp_dir().join(format!("fox-flat-export-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("flat.dxf");
  let p = d.export_flat_params(&path);
  assert_eq!(p["units"], "mm");
  assert!(app.run("sheet.flat_pattern", p).is_some());
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("BEND_EXTENT") && text.contains("BEND_"), "both bend line kinds");
  // inches, no bend lines
  d.flat.units = "in";
  d.flat.center_lines = false;
  d.flat.extent_lines = false;
  assert!(app.run("sheet.flat_pattern", d.export_flat_params(&path)).is_some());
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("$INSUNITS\n70\n1\n"), "inches");
  assert!(!text.contains("\nBEND_UP\n10") && !text.contains("BEND_EXTENT"));
  // STEP takes no DXF options
  d.flat.file_type = "step";
  let step = dir.join("flat.step");
  let p = d.export_flat_params(&step);
  assert!(p.get("units").is_none());
  assert!(app.run("sheet.flat_pattern", p).is_some());
  assert!(std::fs::read_to_string(&step).unwrap().starts_with("ISO-10303-21"));
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unfold_asks_for_a_reference_and_all_bends_then_refold_panel_refolds() {
  let mut app = sheet();
  assert_eq!(flat_ui::refold_panel(&app), None);
  assert!(!panel_commands(&app, Tab::SheetMetal, "MODIFY").contains(&"Refold Faces"));
  app.start(Kind::Unfold);
  // while UNFOLD is open the REFOLD panel shows, greyed
  assert_eq!(flat_ui::refold_panel(&app), Some(false));
  assert_eq!(labels(&app, Tab::SheetMetal).last(), Some(&"REFOLD"));
  assert_eq!(panel_labels(&app, Tab::SheetMetal).last(), Some(&("REFOLD", true)));
  let mut d = app.dialog.take().unwrap();
  assert_eq!(d.heading(), "UNFOLD");
  assert!(show_once(&mut app, &mut d));
  assert!(!d.ok_enabled() && !d.picks(), "OK greyed, no Preview box");
  assert_eq!(d.prompt(false).as_deref(), Some(sheet_flat::UNFOLD_HINT));
  assert_eq!(sheet_flat::UNFOLD_HINT, "Select stationary entity");
  let features = app.s.design.features.len();
  app.s.selection = vec![bottom(&app)];
  assert!(show_once(&mut app, &mut d));
  assert_eq!(d.faces.len(), 1);
  assert!(!d.ok_enabled(), "the bends are still to choose");
  assert!(d.tint_faces(&app.s).is_empty());
  d.flat.all_bends = true;
  assert!(show_once(&mut app, &mut d));
  assert!(d.ok_enabled());
  // the bends preview green; the sheet stays folded until OK
  let tints = d.tint_faces(&app.s);
  assert!(!tints.is_empty() && tints.iter().all(|t| t.2 == sheet_flat::BEND_PREVIEW));
  assert_eq!(app.s.design.features.len(), features);
  assert!(flat_ui::unfolded(&app).is_empty());
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d));
  assert_eq!(app.s.design.features.len(), features + 1);
  assert_eq!(app.s.design.features.last().unwrap().name, "Unfold1");
  // the REFOLD panel lights; Refold runs at once and the panel goes
  assert_eq!(flat_ui::refold_panel(&app), Some(true));
  assert_eq!(panel_labels(&app, Tab::SheetMetal).last(), Some(&("REFOLD", false)));
  Cmd::Flat(FlatAct::Refold).run(&mut app);
  assert!(app.dialog.is_none(), "no dialog");
  assert_eq!(app.s.design.features.len(), features + 2);
  assert!(matches!(app.s.design.features.last().unwrap().kind, fox_engine::model::FeatureKind::Refold { .. }));
  assert_eq!(flat_ui::refold_panel(&app), None);
}
