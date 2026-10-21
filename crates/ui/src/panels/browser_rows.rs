//! The browser's folder rows, as in Fusion: the root component's eye and right-click menu, the
//! Bodies and Origin folders' menus, the Units row's menu, the "(Unsaved)" root label and the
//! document tab's lock, and the Sketches folder closing when a sketch is finished.

use fox_engine::model::BodyId;
use serde_json::json;

use super::{RowCmd, RowItem, browser_edit, menu_item, row_item, row_menu};
use crate::dialogs::Kind;
use crate::icons::Icon;
use crate::{App, marking};

/// The three origin planes as `sketch.create` planes.
const ORIGIN_PLANES: [&str; 3] = ["xy", "xz", "yz"];

// ---------------------------------------------------------------- labels

/// Who is signed in: Fusion puts the account name in the window title; FissionOxide has the
/// machine's user.
pub(crate) fn user_name() -> String {
  std::env::var("USER")
    .or_else(|_| std::env::var("USERNAME"))
    .ok()
    .filter(|u| !u.trim().is_empty())
    .unwrap_or_else(|| "Local user".into())
}

/// Whether the design has never been saved (no hub version, no file).
pub(crate) fn unsaved(app: &App) -> bool {
  app.s.hub.is_none() && app.s.path.is_none()
}

/// The design's name as the tab and title show it: the file's stem, else the design name.
fn doc_name(app: &App) -> String {
  app
    .s
    .path
    .as_ref()
    .and_then(|p| p.file_stem())
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_else(|| app.s.design.name.clone())
}

/// The design's version: the hub's, else v1 for a file.
fn version(app: &App) -> u32 {
  app.s.hub.as_ref().map(|h| h.version).unwrap_or(1)
}

/// The root browser row: "(Unsaved)" until the design is saved, then "Name v1", as in Fusion.
pub(crate) fn root_label(app: &App) -> String {
  if unsaved(app) { "(Unsaved)".into() } else { format!("{} v{}", doc_name(app), version(app)) }
}

/// The document tab: "Untitled" (with a lock) until saved, then "Name v1", starred when dirty.
pub(crate) fn tab_label(app: &App) -> String {
  if unsaved(app) {
    doc_name(app)
  } else {
    let star = if app.s.dirty { "*" } else { "" };
    format!("{} v{}{star}", doc_name(app), version(app))
  }
}

/// The window title, as Fusion's "Untitled* (user) - Autodesk Fusion".
pub(crate) fn window_title(app: &App) -> String {
  let name = if unsaved(app) { doc_name(app) } else { format!("{} v{}", doc_name(app), version(app)) };
  let star = if app.s.dirty { "*" } else { "" };
  format!("{name}{star} ({}) - FissionOxide", user_name())
}

/// The small padlock after an unsaved document's tab name.
pub(crate) fn lock_glyph(p: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
  use egui::{Rect, Stroke, pos2, vec2};
  let body = Rect::from_center_size(pos2(c.x, c.y + 2.0), vec2(9.0, 7.0));
  p.rect_filled(body, 1.0, color);
  // the shackle: an arc drawn as a short polyline
  let s = Stroke::new(1.4, color);
  let (r, cy) = (3.0, body.top() - 1.0);
  let pts: Vec<egui::Pos2> =
    (0..=8).map(|i| i as f32 / 8.0 * std::f32::consts::PI).map(|a| pos2(c.x - r * a.cos(), cy - r * a.sin())).collect();
  p.add(egui::Shape::line(pts, s));
}

// ---------------------------------------------------------------- the root row

/// Every body in the design, in browser order.
fn all_bodies(app: &App) -> Vec<BodyId> {
  app.s.model.bodies.iter().map(|b| b.id).collect()
}

/// Every sketch feature.
fn all_sketches(app: &App) -> Vec<fox_engine::model::FeatureId> {
  app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, fox_engine::model::FeatureKind::Sketch { .. }))
    .map(|f| f.id)
    .collect()
}

/// The root row's eye: lit while anything in the component shows (an empty design shows lit).
pub(crate) fn root_visible(app: &App) -> bool {
  let bodies = all_bodies(app);
  let sketches = all_sketches(app);
  if bodies.is_empty() && sketches.is_empty() {
    return true;
  }
  bodies.iter().any(|b| app.s.body_visible(*b))
    || sketches.iter().any(|s| fox_engine::scene::sketch_visible(&app.s, *s))
}

/// The root row's eye or Show/Hide: hides everything in the component, or shows it all again.
pub(crate) fn root_show_hide(app: &mut App) {
  let show = !root_visible(app);
  for b in all_bodies(app) {
    app.run("body.visible", json!({"body": b.0, "visible": show}));
  }
  for s in all_sketches(app) {
    app.run("sketch.visible", json!({"sketch": s.0, "visible": show}));
  }
  if !show {
    app.s.selection.clear();
  }
}

/// Show All Bodies: every body visible.
fn show_all_bodies(app: &mut App) {
  for b in all_bodies(app) {
    if !app.s.body_visible(b) {
      app.run("body.visible", json!({"body": b.0, "visible": true}));
    }
  }
}

/// Show All Components: every body that belongs to a component visible.
fn show_all_components(app: &mut App) {
  let in_comp: Vec<BodyId> = all_bodies(app).into_iter().filter(|b| app.s.design.component_of(*b).is_some()).collect();
  for b in in_comp {
    if !app.s.body_visible(b) {
      app.run("body.visible", json!({"body": b.0, "visible": true}));
    }
  }
}

/// The root component's menu, as Fusion's. Cloud-only entries (Add To Assembly, Create
/// Selection Set, Texture Map Controls, History, Display Detail Control) are listed greyed.
pub(crate) fn root_menu(app: &App) -> Vec<Option<RowItem>> {
  let visible = root_visible(app);
  let direct = app.s.design.history_off;
  vec![
    row_item(Icon::AddToAssembly, "Add To Assembly", RowCmd::NotYet),
    row_item(Icon::NewDrawing, "Create Drawing", RowCmd::CreateDrawing),
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    row_item(Icon::Configure, "Configure", RowCmd::Configure),
    None,
    row_item(Icon::PhysMaterial, "Physical Material", RowCmd::PhysicalMaterial),
    row_item(Icon::Appearance, "Appearance", RowCmd::Appearance),
    row_item(Icon::Decal, "Texture Map Controls", RowCmd::NotYet),
    row_item(Icon::Properties, "Properties", RowCmd::Properties),
    None,
    row_item(Icon::Save, "Export...", RowCmd::Export),
    row_item(Icon::Save, "Save Copy As", RowCmd::SaveCopyAs),
    row_item(Icon::ConvertMesh, "Save As Mesh", RowCmd::SaveAsMesh),
    row_item(Icon::History, "History", RowCmd::NotYet),
    None,
    row_item(Icon::Display, "Display Detail Control", RowCmd::NotYet),
    row_item(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", RowCmd::ShowHide),
    row_item(Icon::Component, "Show All Components", RowCmd::ShowAllComponents),
    row_item(Icon::Body, "Show All Bodies", RowCmd::ShowAllBodies),
    row_item(Icon::Body, "Opacity Control", RowCmd::Opacity),
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
    None,
    row_item(
      if direct { Icon::History } else { Icon::DirectEdit },
      if direct { "Capture Design History" } else { "Change to Direct Modeling" },
      RowCmd::DirectModeling,
    ),
  ]
}

/// Draws the root menu: as [`row_menu`], with Export... and Opacity Control as sub-menus and
/// Physical Material greyed until a component exists to hold one.
pub(crate) fn root_context_menu(app: &mut App, ui: &mut egui::Ui) {
  let items = root_menu(app);
  let has_component = app.s.design.components.iter().any(|c| !c.bodies.is_empty());
  let bodies = all_bodies(app);
  let mut picked = None;
  for it in &items {
    let Some(it) = it else {
      ui.separator();
      continue;
    };
    match it.cmd {
      RowCmd::NotYet => {
        ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, None))
          .inner
          .on_disabled_hover_text("Not in FissionOxide yet");
      }
      RowCmd::PhysicalMaterial if !has_component => {
        ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, None))
          .inner
          .on_disabled_hover_text("Create a component from a body to give it a physical material");
      }
      RowCmd::Export => {
        // the leading spaces leave room for the icon, as menu_item does
        let r = ui.menu_button(format!("       {}", it.label), |ui| {
          for f in fox_engine::exchange::FORMATS.iter().filter(|f| f.export) {
            if ui.button(f.name).clicked() {
              ui.close();
              picked = Some(RowCmd::Export);
              app.pending_export = Some(f.id);
            }
          }
        });
        crate::icons::draw_color(
          ui.painter(),
          egui::Rect::from_center_size(r.response.rect.left_center() + egui::vec2(13.0, 0.0), egui::Vec2::splat(15.0)),
          it.icon,
        );
      }
      RowCmd::Opacity => {
        browser_edit::opacity_menu(app, ui, &bodies);
      }
      RowCmd::ShowHide => {
        if menu_item(ui, it.icon, &it.label, Some("V")).clicked() {
          ui.close();
          picked = Some(it.cmd);
        }
      }
      _ => {
        if menu_item(ui, it.icon, &it.label, None).clicked() {
          ui.close();
          picked = Some(it.cmd);
        }
      }
    }
  }
  if let Some(cmd) = picked {
    run_root_cmd(app, cmd);
  }
}

/// Runs a root row menu command.
pub(crate) fn run_root_cmd(app: &mut App, cmd: RowCmd) {
  let bodies = all_bodies(app);
  let name = doc_name(app);
  match cmd {
    RowCmd::CreateDrawing => app.open_drawing_workspace(),
    RowCmd::Configure => crate::config_ui::open_table(),
    RowCmd::PhysicalMaterial => {
      // physical materials belong to components in FissionOxide
      let in_comp: Vec<fox_engine::Sel> = bodies
        .iter()
        .filter(|b| app.s.design.component_of(**b).is_some())
        .map(|b| fox_engine::Sel::Body { body: *b })
        .collect();
      app.s.selection = in_comp;
      app.start(Kind::Material);
    }
    RowCmd::Appearance => {
      app.s.selection = bodies.iter().map(|b| fox_engine::Sel::Body { body: *b }).collect();
      app.start(Kind::Form(&crate::forms::APPEARANCE));
    }
    RowCmd::Properties => {
      app.s.selection = bodies.iter().map(|b| fox_engine::Sel::Body { body: *b }).collect();
      app.start(Kind::Inspect);
    }
    RowCmd::Export => {
      if let Some(id) = app.pending_export.take() {
        app.export(id);
      }
    }
    RowCmd::SaveCopyAs => super::save_component_copy(app, &name, &bodies),
    RowCmd::SaveAsMesh => save_as_mesh(app, &name, &bodies),
    RowCmd::ShowHide => root_show_hide(app),
    RowCmd::ShowAllComponents => show_all_components(app),
    RowCmd::ShowAllBodies => show_all_bodies(app),
    RowCmd::FindInWindow => {
      if bodies.is_empty() {
        app.fit_view();
      } else {
        marking::find_in_window(app, &bodies, None);
      }
    }
    RowCmd::DirectModeling => crate::config_ui::toggle_history(app),
    _ => {}
  }
}

/// Save As Mesh on the root: every body to one STL, OBJ or 3MF file.
fn save_as_mesh(app: &mut App, name: &str, bodies: &[BodyId]) {
  if bodies.is_empty() {
    app.status = Some((format!("{name} has no bodies to save"), true));
    return;
  }
  let Some(p) = rfd::FileDialog::new()
    .add_filter("STL", &["stl"])
    .add_filter("OBJ", &["obj"])
    .add_filter("3MF", &["3mf"])
    .set_file_name(format!("{name}.stl"))
    .save_file()
  else {
    return;
  };
  let ids: Vec<u32> = bodies.iter().map(|b| b.0).collect();
  if app.run("export", json!({"path": p, "bodies": ids})).is_some() {
    app.status = Some((format!("Saved {name} as {}", p.display()), false));
  }
}

// ---------------------------------------------------------------- the Bodies folder

/// The Bodies folder's menu, as Fusion's (New Group and Create Selection Set greyed).
pub(crate) fn bodies_folder_menu(app: &App) -> Vec<Option<RowItem>> {
  let bodies = all_bodies(app);
  let visible = bodies.iter().any(|b| app.s.body_visible(*b));
  let locked = !bodies.is_empty() && bodies.iter().all(|b| app.ctx_menu.unselectable.contains(b));
  vec![
    row_item(Icon::Folder, "New Group", RowCmd::NotYet),
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    None,
    row_item(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", RowCmd::ShowHide),
    row_item(Icon::Eye, "Show All", RowCmd::ShowAll),
    None,
    row_item(Icon::Select, if locked { "Selectable" } else { "Selectable/Unselectable" }, RowCmd::Selectable),
  ]
}

/// Runs a Bodies folder menu command.
pub(crate) fn run_bodies_folder_cmd(app: &mut App, cmd: RowCmd) {
  let bodies = all_bodies(app);
  match cmd {
    RowCmd::ShowHide => marking::show_hide(app, &bodies),
    RowCmd::ShowAll => show_all_bodies(app),
    RowCmd::Selectable => {
      let locked = !bodies.is_empty() && bodies.iter().all(|b| app.ctx_menu.unselectable.contains(b));
      let m = &mut app.ctx_menu;
      m.unselectable.retain(|b| !bodies.contains(b));
      if !locked {
        m.unselectable.extend(&bodies);
        app.s.selection.retain(|s| {
          !matches!(s, fox_engine::Sel::Body { body } | fox_engine::Sel::Face { body, .. } | fox_engine::Sel::Edge { body, .. } if bodies.contains(body))
        });
      }
    }
    _ => {}
  }
}

/// Draws a folder's menu and runs what was clicked.
pub(crate) fn folder_context_menu(ui: &mut egui::Ui, items: &[Option<RowItem>], run: impl FnOnce(RowCmd)) {
  if let Some(cmd) = row_menu(ui, items) {
    run(cmd);
  }
}

// ---------------------------------------------------------------- the Origin folder

/// Whether the three origin planes are shown in the viewport: by the Origin folder's eye, or
/// each by its own Show/Hide.
pub(crate) fn origin_planes_shown(app: &App) -> bool {
  app.origin_shown || ORIGIN_PLANES.iter().all(|p| app.shown_planes.contains(&json!(p)))
}

fn set_origin_planes(app: &mut App, shown: bool) {
  app.shown_planes.retain(|p| !ORIGIN_PLANES.iter().any(|o| *p == json!(o)));
  if shown {
    app.shown_planes.extend(ORIGIN_PLANES.iter().map(|p| json!(p)));
  }
}

/// The Origin folder's eye: hidden to start, as in Fusion; shown, the viewport draws the
/// origin's planes and axes (`App::origin_shown`).
pub(crate) fn origin_show_hide(app: &mut App) {
  app.origin_shown = !app.origin_shown;
}

/// The Origin folder's menu, as Fusion's (Create Selection Set greyed).
pub(crate) fn origin_folder_menu(app: &App) -> Vec<Option<RowItem>> {
  let visible = app.origin_shown;
  let planes = origin_planes_shown(app);
  let axes = app.objects.origins;
  vec![
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    None,
    row_item(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", RowCmd::ShowHide),
    row_item(Icon::Eye, "Show All", RowCmd::ShowAll),
    row_item(if planes { Icon::EyeOff } else { Icon::Eye }, "Show/Hide Planes", RowCmd::ShowHidePlanes),
    row_item(if axes { Icon::EyeOff } else { Icon::Eye }, "Show/Hide Axes", RowCmd::ShowHideAxes),
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
  ]
}

/// Runs an Origin folder menu command. Show/Hide Planes shows the three planes on their own
/// (the folder stays hidden); Show/Hide Axes is Object Visibility's Origins switch.
pub(crate) fn run_origin_folder_cmd(app: &mut App, cmd: RowCmd) {
  match cmd {
    RowCmd::ShowHide => origin_show_hide(app),
    RowCmd::ShowAll => {
      app.origin_shown = true;
      app.objects.origins = true;
    }
    RowCmd::ShowHidePlanes => {
      if app.origin_shown {
        // the folder's eye showed them: hide the folder, keep nothing but the planes
        app.origin_shown = false;
        set_origin_planes(app, false);
      } else {
        let shown = origin_planes_shown(app);
        set_origin_planes(app, !shown);
      }
    }
    RowCmd::ShowHideAxes => app.objects.origins = !app.objects.origins,
    RowCmd::FindInWindow => {
      let pl = fox_engine::model::OriginPlane::Xy.plane(app.s.design.frames());
      marking::find_plane_in_window(app, &pl);
    }
    _ => {}
  }
}

// ---------------------------------------------------------------- the Units row

/// The Units row's menu: Change Active Units, as Fusion's.
pub(crate) fn units_menu() -> Vec<Option<RowItem>> {
  vec![row_item(Icon::Measure, "Change Active Units", RowCmd::ChangeUnits)]
}

/// Runs a Units row menu command: Change Active Units opens the row's units popup.
pub(crate) fn run_units_cmd(ctx: &egui::Context, cmd: RowCmd) {
  if cmd == RowCmd::ChangeUnits {
    egui::Popup::open_id(ctx, crate::units::popup_id());
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::{Sel, Session};
  use serde_json::json;

  use super::*;

  fn labels(items: &[Option<RowItem>]) -> Vec<String> {
    items.iter().map(|i| i.as_ref().map(|i| i.label.clone()).unwrap_or_else(|| "-".into())).collect()
  }

  fn boxed() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    assert_eq!(app.s.model.bodies.len(), 1, "no box");
    app
  }

  #[test]
  fn root_labels_follow_fusion() {
    let mut app = App::with_session(Session::default());
    assert_eq!(root_label(&app), "(Unsaved)");
    assert_eq!(tab_label(&app), "Untitled");
    let user = user_name();
    assert_eq!(window_title(&app), format!("Untitled ({user}) - FissionOxide"));
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    assert!(app.s.dirty);
    assert_eq!(window_title(&app), format!("Untitled* ({user}) - FissionOxide"));
    assert_eq!(tab_label(&app), "Untitled", "the tab shows the lock, not a star, until saved");
    // saved to a file: the name and version show, the tab stars its unsaved changes
    app.s.path = Some(std::path::PathBuf::from("/tmp/Bracket.fox"));
    assert_eq!(root_label(&app), "Bracket v1");
    assert_eq!(tab_label(&app), "Bracket v1*");
    assert_eq!(window_title(&app), format!("Bracket v1* ({user}) - FissionOxide"));
    app.s.dirty = false;
    assert_eq!(tab_label(&app), "Bracket v1");
  }

  #[test]
  fn root_menu_is_fusions() {
    let app = boxed();
    assert_eq!(
      labels(&root_menu(&app)),
      [
        "Add To Assembly",
        "Create Drawing",
        "Create Selection Set",
        "Configure",
        "-",
        "Physical Material",
        "Appearance",
        "Texture Map Controls",
        "Properties",
        "-",
        "Export...",
        "Save Copy As",
        "Save As Mesh",
        "History",
        "-",
        "Display Detail Control",
        "Show/Hide",
        "Show All Components",
        "Show All Bodies",
        "Opacity Control",
        "Find in Window",
        "-",
        "Change to Direct Modeling",
      ]
    );
    let cloud: Vec<&str> = root_menu(&app)
      .iter()
      .flatten()
      .filter(|i| i.cmd == RowCmd::NotYet)
      .map(|i| i.label.as_str())
      .collect::<Vec<_>>();
    assert_eq!(
      cloud,
      ["Add To Assembly", "Create Selection Set", "Texture Map Controls", "History", "Display Detail Control"]
    );
  }

  #[test]
  fn root_eye_hides_and_shows_everything() {
    let mut app = boxed();
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
    app.finish_sketch();
    let body = app.s.model.bodies[0].id;
    assert!(root_visible(&app));
    app.s.selection = vec![Sel::Body { body }];
    root_show_hide(&mut app);
    assert!(!root_visible(&app));
    assert!(!app.s.body_visible(body));
    assert!(!fox_engine::scene::sketch_visible(&app.s, sk));
    assert!(app.s.selection.is_empty(), "hidden things are no longer selected");
    root_show_hide(&mut app);
    assert!(app.s.body_visible(body) && fox_engine::scene::sketch_visible(&app.s, sk));
    // an empty design's root shows a lit eye
    assert!(root_visible(&App::with_session(Session::default())));
  }

  #[test]
  fn root_commands_run() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    app.run("body.visible", json!({"body": body.0, "visible": false}));
    run_root_cmd(&mut app, RowCmd::ShowAllBodies);
    assert!(app.s.body_visible(body));
    app.run("body.visible", json!({"body": body.0, "visible": false}));
    run_root_cmd(&mut app, RowCmd::ShowAllComponents);
    assert!(!app.s.body_visible(body), "Show All Components leaves root bodies alone");
    app.run("component.create", json!({"name": "Part", "bodies": [body.0]}));
    run_root_cmd(&mut app, RowCmd::ShowAllComponents);
    assert!(app.s.body_visible(body));
    run_root_cmd(&mut app, RowCmd::Appearance);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Form(&crate::forms::APPEARANCE)));
    run_root_cmd(&mut app, RowCmd::Properties);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Inspect));
    run_root_cmd(&mut app, RowCmd::PhysicalMaterial);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Material));
    run_root_cmd(&mut app, RowCmd::FindInWindow);
    assert!(!app.s.design.history_off);
    run_root_cmd(&mut app, RowCmd::DirectModeling);
    assert!(app.s.design.history_off);
    assert!(labels(&root_menu(&app)).contains(&"Capture Design History".into()));
    run_root_cmd(&mut app, RowCmd::DirectModeling);
    assert!(!app.s.design.history_off);
    run_root_cmd(&mut app, RowCmd::CreateDrawing);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::NewDrawing), "no drawing yet: New Drawing opens");
  }

  #[test]
  fn bodies_folder_menu_is_fusions() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    assert_eq!(
      labels(&bodies_folder_menu(&app)),
      ["New Group", "Create Selection Set", "-", "Show/Hide", "Show All", "-", "Selectable/Unselectable"]
    );
    run_bodies_folder_cmd(&mut app, RowCmd::ShowHide);
    assert!(!app.s.body_visible(body));
    run_bodies_folder_cmd(&mut app, RowCmd::ShowAll);
    assert!(app.s.body_visible(body));
    app.s.selection = vec![Sel::Body { body }];
    run_bodies_folder_cmd(&mut app, RowCmd::Selectable);
    assert_eq!(app.ctx_menu.unselectable, vec![body]);
    assert!(app.s.selection.is_empty());
    assert!(labels(&bodies_folder_menu(&app)).contains(&"Selectable".into()));
    run_bodies_folder_cmd(&mut app, RowCmd::Selectable);
    assert!(app.ctx_menu.unselectable.is_empty());
  }

  #[test]
  fn origin_folder_starts_hidden_and_its_menu_is_fusions() {
    let mut app = App::with_session(Session::default());
    assert!(!app.origin_shown, "Origin starts hidden, as in Fusion");
    assert_eq!(
      labels(&origin_folder_menu(&app)),
      ["Create Selection Set", "-", "Show/Hide", "Show All", "Show/Hide Planes", "Show/Hide Axes", "Find in Window"]
    );
    origin_show_hide(&mut app);
    assert!(app.origin_shown && origin_planes_shown(&app));
    assert!(app.shown_planes.is_empty(), "the folder's eye draws the planes on its own");
    origin_show_hide(&mut app);
    assert!(!app.origin_shown && !origin_planes_shown(&app));
    run_origin_folder_cmd(&mut app, RowCmd::ShowHidePlanes);
    assert!(origin_planes_shown(&app) && !app.origin_shown, "the planes alone, the folder still hidden");
    assert_eq!(app.shown_planes.len(), 3);
    run_origin_folder_cmd(&mut app, RowCmd::ShowHidePlanes);
    assert!(!origin_planes_shown(&app) && app.shown_planes.is_empty());
    origin_show_hide(&mut app);
    run_origin_folder_cmd(&mut app, RowCmd::ShowHidePlanes);
    assert!(!app.origin_shown && !origin_planes_shown(&app), "Show/Hide Planes on a shown folder hides them");
    let axes = app.objects.origins;
    run_origin_folder_cmd(&mut app, RowCmd::ShowHideAxes);
    assert_eq!(app.objects.origins, !axes);
    run_origin_folder_cmd(&mut app, RowCmd::ShowAll);
    assert!(app.origin_shown && origin_planes_shown(&app) && app.objects.origins);
    run_origin_folder_cmd(&mut app, RowCmd::FindInWindow);
    assert!(app.status.as_ref().is_none_or(|(_, err)| !err), "{:?}", app.status);
  }

  #[test]
  fn units_row_menu_opens_the_units_popup() {
    let ctx = egui::Context::default();
    assert_eq!(labels(&units_menu()), ["Change Active Units"]);
    assert!(!egui::Popup::is_id_open(&ctx, crate::units::popup_id()));
    run_units_cmd(&ctx, RowCmd::ChangeUnits);
    assert!(egui::Popup::is_id_open(&ctx, crate::units::popup_id()));
  }

  #[test]
  fn finish_sketch_closes_the_sketches_folder_and_selects_nothing() {
    let mut app = App::with_session(Session::default());
    assert_eq!(app.browser_sketches, None);
    app.create_sketch(json!("xy"));
    assert_eq!(app.browser_sketches, Some(true), "the folder opens on the sketch being edited");
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
    app.s.selection = vec![Sel::Feature { feature: sk }];
    app.finish_sketch();
    assert_eq!(app.browser_sketches, Some(false), "Finish Sketch collapses the folder");
    assert!(app.s.selection.is_empty() && app.s.edit_sketch.is_none(), "nothing stays highlighted");
    app.browser_sketches = None;
    app.edit_sketch(sk);
    assert_eq!(app.browser_sketches, Some(true));
  }
}
