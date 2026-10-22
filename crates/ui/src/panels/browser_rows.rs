//! The browser's folder rows, as in Fusion: the root component's eye and right-click menu, the
//! Bodies and Origin folders' menus, the Units row's menu, the "(Unsaved)" root label and the
//! document tab's lock, and the Sketches folder closing when a sketch is finished.

use fox_engine::model::BodyId;
use serde_json::json;

use super::{RowCmd, RowItem, browser_edit, menu_item, row_item, row_key, row_menu};
use crate::dialogs::Kind;
use crate::icons::Icon;
use crate::origin::{self, OriginItem};
use crate::{App, RenameTarget, marking, theme};

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
  crate::documents::unsaved(&app.s)
}

/// The design's name as the tab and title show it: the file's stem, else the design name.
/// The document's name: its file's stem once saved, else the design's name.
pub(crate) fn doc_name(app: &App) -> String {
  crate::documents::doc_name(&app.s)
}

/// The design's version: the hub's, else v1 for a file.
fn version(app: &App) -> u32 {
  app.s.hub.as_ref().map(|h| h.version).unwrap_or(1)
}

/// The root browser row: "(Unsaved)" until the design is saved, then "Name v1", as in Fusion.
pub(crate) fn root_label(app: &App) -> String {
  if unsaved(app) { "(Unsaved)".into() } else { format!("{} v{}", doc_name(app), version(app)) }
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
      toggle_selectable(app, &bodies, locked);
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

// ---------------------------------------------------------------- a surface body row

/// A surface body row's menu, as Fusion's: no Create Components from Bodies, and Physical
/// Material enabled. Move to Group, Create Selection Set, Texture Map Controls, Cut and Display
/// Detail Control are listed greyed.
pub(crate) fn surface_body_menu(app: &App, bid: BodyId) -> Vec<Option<RowItem>> {
  let visible = app.s.body_visible(bid);
  let locked = app.ctx_menu.unselectable.contains(&bid);
  let isolated = app.ctx_menu.isolate.is_some();
  vec![
    row_key(Icon::Move, "Move/Copy", "M", RowCmd::MoveCopy),
    row_item(Icon::Folder, "Move to Group", RowCmd::NotYet),
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    row_item(Icon::Configure, "Configure", RowCmd::Configure),
    row_item(Icon::PhysMaterial, "Physical Material", RowCmd::PhysicalMaterial),
    row_key(Icon::Appearance, "Appearance", "A", RowCmd::Appearance),
    row_item(Icon::Decal, "Texture Map Controls", RowCmd::NotYet),
    row_item(Icon::Properties, "Properties", RowCmd::Properties),
    row_item(Icon::ConvertMesh, "Save As Mesh", RowCmd::SaveAsMesh),
    None,
    row_key(Icon::Duplicate, "Copy", "Ctrl+C", RowCmd::Copy),
    row_key(Icon::SkTrim, "Cut", "Ctrl+X", RowCmd::NotYet),
    row_key(Icon::Remove, "Delete", "Del", RowCmd::Delete),
    row_item(Icon::RemoveBody, "Remove", RowCmd::Remove),
    row_item(Icon::Params, "Rename", RowCmd::Rename),
    None,
    row_item(Icon::Display, "Display Detail Control", RowCmd::NotYet),
    None,
    row_key(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", "V", RowCmd::ShowHide),
    row_item(Icon::Select, if locked { "Selectable" } else { "Selectable/Unselectable" }, RowCmd::Selectable),
    None,
    row_item(Icon::Body, "Opacity Control", RowCmd::Opacity),
    row_item(Icon::Body, if isolated { "Unisolate" } else { "Isolate" }, RowCmd::Isolate),
    None,
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
  ]
}

/// Draws a surface body row's menu ([`surface_body_menu`]), Opacity Control as its sub-menu,
/// and runs what was clicked.
pub(crate) fn surface_body_context_menu(app: &mut App, ui: &mut egui::Ui, row: egui::Id, bid: BodyId, name: &str) {
  let items = surface_body_menu(app, bid);
  let mut picked = None;
  for it in &items {
    let Some(it) = it else {
      ui.separator();
      continue;
    };
    match it.cmd {
      RowCmd::NotYet => {
        ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, it.key))
          .inner
          .on_disabled_hover_text("Not in FissionOxide yet");
      }
      RowCmd::Opacity => {
        let browser_edit::Dragged::Bodies(bodies) = browser_edit::dragged_bodies(app, bid) else { continue };
        browser_edit::opacity_menu(app, ui, &bodies);
      }
      _ => {
        if menu_item(ui, it.icon, &it.label, it.key).clicked() {
          ui.close();
          picked = Some(it.cmd);
        }
      }
    }
  }
  match picked {
    Some(RowCmd::Rename) => browser_edit::begin(ui, row, RenameTarget::Body(bid.0), name),
    Some(cmd) => run_surface_body_cmd(app, ui.ctx(), bid, cmd),
    None => {}
  }
}

/// Runs a surface body row's menu command. Like Fusion's, Copy and Selectable/Unselectable act
/// on every selected body when the row is one of them.
pub(crate) fn run_surface_body_cmd(app: &mut App, ctx: &egui::Context, bid: BodyId, cmd: RowCmd) {
  let only = vec![fox_engine::Sel::Body { body: bid }];
  let browser_edit::Dragged::Bodies(bodies) = browser_edit::dragged_bodies(app, bid) else { return };
  let start = |app: &mut App, kind: Kind| {
    app.s.selection = only.clone();
    app.start(kind);
  };
  match cmd {
    RowCmd::MoveCopy => start(app, Kind::Move),
    RowCmd::Configure => crate::config_ui::open_table(),
    RowCmd::PhysicalMaterial => start(app, Kind::Material),
    RowCmd::Appearance => start(app, Kind::Form(&crate::forms::APPEARANCE)),
    RowCmd::Properties => start(app, Kind::Inspect),
    RowCmd::SaveAsMesh => marking::save_as_mesh(app, bid),
    RowCmd::Copy => {
      app.s.selection = bodies.iter().map(|b| fox_engine::Sel::Body { body: *b }).collect();
      crate::clipboard::copy(app, ctx);
    }
    RowCmd::Delete => {
      app.s.selection = only;
      app.delete_selection();
    }
    RowCmd::Remove => _ = app.run("body.remove", json!({"bodies": [bid.0]})),
    RowCmd::ShowHide => marking::show_hide(app, &[bid]),
    RowCmd::Selectable => toggle_selectable(app, &bodies, app.ctx_menu.unselectable.contains(&bid)),
    RowCmd::Isolate => {
      app.ctx_menu.isolate = if app.ctx_menu.isolate.is_some() { None } else { Some(vec![bid]) };
    }
    RowCmd::FindInWindow => marking::find_in_window(app, &[bid], None),
    _ => {}
  }
}

/// Makes `bodies` selectable again when `locked`, else unselectable (and drops them from the
/// selection).
fn toggle_selectable(app: &mut App, bodies: &[BodyId], locked: bool) {
  let m = &mut app.ctx_menu;
  m.unselectable.retain(|b| !bodies.contains(b));
  if !locked {
    m.unselectable.extend(bodies);
    app.s.selection.retain(|s| {
      !matches!(s, fox_engine::Sel::Body { body } | fox_engine::Sel::Face { body, .. } | fox_engine::Sel::Edge { body, .. } if bodies.contains(body))
    });
  }
}

// ---------------------------------------------------------------- the Origin folder

/// The Origin folder's eye: hidden to start, as in Fusion; shown, the viewport draws the
/// origin's point, axes and planes whose own eyes are on (`App::origin_shown`).
pub(crate) fn origin_show_hide(app: &mut App) {
  app.origin_shown = !app.origin_shown;
}

/// The Origin folder's menu, as Fusion's (Create Selection Set greyed).
pub(crate) fn origin_folder_menu(app: &App) -> Vec<Option<RowItem>> {
  let visible = app.origin_shown;
  let planes = origin::planes_shown(app);
  let axes = origin::axes_shown(app);
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

/// Runs an Origin folder menu command. Show/Hide Planes and Show/Hide Axes toggle the three
/// planes' or axes' own eyes (they show only while the folder's eye is on, as in Fusion); Show
/// All turns the folder and every child on.
pub(crate) fn run_origin_folder_cmd(app: &mut App, cmd: RowCmd) {
  match cmd {
    RowCmd::ShowHide => origin_show_hide(app),
    RowCmd::ShowAll => {
      app.origin_shown = true;
      app.objects.show_origin();
      for item in OriginItem::ALL {
        origin::set_eye(app, item, true);
      }
    }
    RowCmd::ShowHidePlanes => {
      let shown = origin::planes_shown(app);
      origin::set_planes(app, !shown);
    }
    RowCmd::ShowHideAxes => {
      let shown = origin::axes_shown(app);
      origin::set_axes(app, !shown);
    }
    RowCmd::FindInWindow => {
      let pl = fox_engine::model::OriginPlane::Xy.plane(app.s.design.frames());
      marking::find_plane_in_window(app, &pl);
    }
    _ => {}
  }
}

/// The Origin folder's children, as Fusion's: O, X, Y, Z, XY, XZ, YZ, each with its own eye
/// (greyed while the folder is hidden). A click selects the item (status "XZ Plane") whether or
/// not it shows; a double-click on a plane sketches on it. Hovering a row lights its item in
/// the canvas as hovering the item there does.
pub(crate) fn origin_children(app: &mut App, ui: &mut egui::Ui, folder: egui::Id) {
  for item in OriginItem::ALL {
    let picked = origin::picked(app) == Some(item);
    let eye = origin::eye(app, item);
    let ev = super::tree_row(
      ui,
      folder.with(item.label()),
      2,
      None,
      Some(eye),
      item.icon(),
      item.label(),
      picked,
      theme::TEXT,
    );
    if ev.eye {
      origin::toggle_eye(app, item);
    }
    if ev.response.as_ref().is_some_and(|r| r.hovered()) {
      app.row_hover = Some(origin::RowHover::Origin(item));
    }
    if let Some(r) = &ev.response
      && !app.origin_shown
    {
      // the child's eye is greyed while the folder's is off
      ui.painter().rect_filled(super::eye_rect(r.rect, 2), 0.0, egui::Color32::from_white_alpha(150));
    }
    let plane = item.sketch_plane();
    if ev.double_clicked
      && let Some(plane) = &plane
    {
      // the first click already started it if Create Sketch was waiting for a plane
      if app.s.edit_sketch.is_none() {
        app.create_sketch(plane.clone());
      }
    } else if ev.clicked {
      match &plane {
        Some(plane) => super::pick_plane(app, plane.clone()),
        None => origin::pick(app, item),
      }
    }
    if let Some(r) = ev.response {
      r.context_menu(|ui| match &plane {
        Some(plane) => {
          let plane = plane.as_str().unwrap_or("xy");
          let items = super::origin_plane_menu(app, plane);
          if let Some(cmd) = row_menu(ui, &items) {
            super::run_origin_plane_cmd(app, plane, cmd);
          }
        }
        None => {
          let items = origin_item_menu(app, item);
          if let Some(cmd) = row_menu(ui, &items) {
            run_origin_item_cmd(app, item, cmd);
          }
        }
      });
    }
  }
}

/// An origin point or axis row's menu: its eye, as Show/Hide.
pub(crate) fn origin_item_menu(app: &App, item: OriginItem) -> Vec<Option<RowItem>> {
  let shown = origin::eye(app, item);
  vec![row_item(if shown { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", RowCmd::ShowHide)]
}

pub(crate) fn run_origin_item_cmd(app: &mut App, item: OriginItem, cmd: RowCmd) {
  if cmd == RowCmd::ShowHide {
    origin::toggle_eye(app, item);
  }
}

// ---------------------------------------------------------------- construction planes

/// A construction plane row's menu, as Fusion's.
pub(crate) fn construction_plane_menu(app: &App, fid: fox_engine::model::FeatureId) -> Vec<Option<RowItem>> {
  let shown = crate::construction::eye(app, fid);
  vec![
    row_item(Icon::Folder, "Move to Group", RowCmd::NotYet),
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    None,
    row_item(Icon::Sketch, "Create Sketch", RowCmd::CreateSketch),
    row_item(Icon::Plane, "Offset Plane", RowCmd::OffsetPlane),
    None,
    row_item(Icon::Plane, "Edit Construction Plane", RowCmd::EditFeature),
    row_item(Icon::Configure, "Configure", RowCmd::Configure),
    None,
    row_key(Icon::Remove, "Delete", "Del", RowCmd::Delete),
    row_item(Icon::Params, "Rename", RowCmd::Rename),
    None,
    row_item(Icon::LookAt, "Look At", RowCmd::LookAt),
    row_key(if shown { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", "V", RowCmd::ShowHide),
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
    row_item(Icon::History, "Find in Timeline", RowCmd::FindInTimeline),
  ]
}

/// Runs a construction plane row's menu command (Rename is begun in the row itself).
pub(crate) fn run_construction_plane_cmd(app: &mut App, fid: fox_engine::model::FeatureId, cmd: RowCmd) {
  let plane = json!({"feature": fid.0});
  let pl = app.s.model.planes.get(&fid).copied();
  match cmd {
    RowCmd::CreateSketch => app.create_sketch(plane),
    RowCmd::OffsetPlane => {
      app.s.selection.clear();
      origin::clear(app);
      app.start(Kind::OffsetPlane);
      let label = app.s.design.feature(fid).map(|f| f.name.clone()).unwrap_or_default();
      if let Some(d) = app.dialog.as_mut() {
        d.use_plane(&mut app.s, plane, label);
      }
    }
    RowCmd::EditFeature => super::edit_feature(app, fid),
    RowCmd::Configure => {
      // the plane's suppression becomes a column of the configuration table
      if let Some(first) = app.s.design.configurations.list.first().map(|c| c.name.clone()) {
        let suppressed = app.s.design.feature(fid).is_some_and(|f| f.suppressed);
        app.run("config.set", json!({"configuration": first, "feature": fid.0, "suppressed": suppressed}));
      }
      crate::config_ui::open_table();
    }
    RowCmd::Delete => {
      if crate::construction::picked(app) == Some(fid) {
        origin::clear(app);
      }
      app.run("feature.delete", json!({"feature": fid.0}));
    }
    RowCmd::LookAt => {
      if let Some(pl) = pl {
        app.look_at_plane(&pl);
      }
    }
    RowCmd::ShowHide => crate::construction::toggle_eye(app, fid),
    RowCmd::FindInWindow => {
      if let Some(pl) = pl {
        marking::find_plane_in_window(app, &pl);
      }
    }
    RowCmd::FindInTimeline => {
      origin::clear(app);
      app.s.selection = vec![fox_engine::Sel::Feature { feature: fid }];
    }
    _ => {}
  }
}

// ---------------------------------------------------------------- the Units row

/// The Units row's menu: Change Active Units, as Fusion's.
pub(crate) fn units_menu() -> Vec<Option<RowItem>> {
  vec![row_item(Icon::Measure, "Change Active Units", RowCmd::ChangeUnits)]
}

/// Runs a Units row menu command: Change Active Units opens its docked dialog.
pub(crate) fn run_units_cmd(app: &mut App, cmd: RowCmd) {
  if cmd == RowCmd::ChangeUnits {
    crate::doc_dialogs::open_units(app);
  }
}

// ---------------------------------------------------------------- the Part Design row

/// The Part Design row's menu: Convert Design Type, as Fusion's.
pub(crate) fn design_type_menu() -> Vec<Option<RowItem>> {
  vec![row_item(Icon::Pencil, "Convert Design Type", RowCmd::ConvertDesignType)]
}

/// Runs a Part Design row menu command: Convert Design Type opens its docked dialog.
pub(crate) fn run_design_type_cmd(app: &mut App, cmd: RowCmd) {
  if cmd == RowCmd::ConvertDesignType {
    crate::doc_dialogs::open_convert(app);
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

  /// The Construction folder lists a plane once its Offset Plane is OK'd (not while previewed);
  /// the plane row's menu is Fusion's, and its commands act on the plane.
  #[test]
  fn construction_plane_rows_follow_fusion() {
    let mut app = App::with_session(Session::default());
    app.start(Kind::OffsetPlane);
    assert_eq!(app.s.model.planes.len(), 1, "previewed");
    assert!(crate::construction::planes(&app).is_empty(), "not in the browser before OK");
    app.dialog = None;
    let planes = crate::construction::planes(&app);
    assert_eq!(planes.len(), 1);
    let fid = planes[0].0;
    assert!(app.construction_shown, "the folder's eye is on");
    assert_eq!(
      labels(&construction_plane_menu(&app, fid)),
      [
        "Move to Group",
        "Create Selection Set",
        "-",
        "Create Sketch",
        "Offset Plane",
        "-",
        "Edit Construction Plane",
        "Configure",
        "-",
        "Delete",
        "Rename",
        "-",
        "Look At",
        "Show/Hide",
        "Find in Window",
        "Find in Timeline",
      ]
    );
    let keys: Vec<Option<&str>> = construction_plane_menu(&app, fid).iter().flatten().map(|i| i.key).collect();
    assert_eq!(keys.iter().flatten().collect::<Vec<_>>(), [&"Del", &"V"]);
    // Show/Hide is the row's eye
    run_construction_plane_cmd(&mut app, fid, RowCmd::ShowHide);
    assert!(!crate::construction::eye(&app, fid));
    run_construction_plane_cmd(&mut app, fid, RowCmd::ShowHide);
    assert!(crate::construction::eye(&app, fid));
    run_construction_plane_cmd(&mut app, fid, RowCmd::LookAt);
    run_construction_plane_cmd(&mut app, fid, RowCmd::FindInWindow);
    run_construction_plane_cmd(&mut app, fid, RowCmd::FindInTimeline);
    assert_eq!(app.s.selection, vec![Sel::Feature { feature: fid }]);
    // Offset Plane starts from this plane: a second plane 10 mm past the first
    run_construction_plane_cmd(&mut app, fid, RowCmd::OffsetPlane);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::OffsetPlane));
    let made = app.dialog.as_ref().and_then(|d| d.previewed_plane()).expect("previewed");
    assert!((app.s.model.planes[&made].origin.z - 20.0).abs() < 1e-9, "{:?}", app.s.model.planes[&made]);
    app.cancel_dialog();
    run_construction_plane_cmd(&mut app, fid, RowCmd::EditFeature);
    assert!(app.dialog.as_ref().is_some_and(|d| d.editing == Some(fid)));
    app.cancel_dialog();
    run_construction_plane_cmd(&mut app, fid, RowCmd::CreateSketch);
    assert!(app.s.edit_sketch.is_some());
    app.finish_sketch();
    run_construction_plane_cmd(&mut app, fid, RowCmd::Delete);
    assert!(app.s.design.feature(fid).is_none());
  }

  /// V shows or hides what is picked: a construction plane (it stays picked), an origin item, a
  /// selected sketch; with nothing to show or hide it says what to select.
  #[test]
  fn v_toggles_the_picked_items_visibility() {
    let mut app = App::with_session(Session::default());
    let v = app.run("plane.offset", json!({"plane": "xy", "offset": 5})).unwrap();
    let fid = fox_engine::model::FeatureId(v["feature"].as_u64().unwrap() as u32);
    super::super::pick_plane(&mut app, json!({"feature": fid.0}));
    app.toggle_visibility();
    assert!(!crate::construction::eye(&app, fid));
    assert_eq!(crate::construction::picked(&app), Some(fid), "still selected");
    app.toggle_visibility();
    assert!(crate::construction::eye(&app, fid));
    origin::pick(&mut app, OriginItem::Y);
    app.toggle_visibility();
    assert!(!origin::eye(&app, OriginItem::Y));
    origin::clear(&mut app);
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.unwrap();
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [5, 5]}));
    app.finish_sketch();
    app.s.selection = vec![Sel::Feature { feature: sk }];
    let shown = fox_engine::scene::sketch_visible(&app.s, sk);
    app.toggle_visibility();
    assert_ne!(fox_engine::scene::sketch_visible(&app.s, sk), shown);
    app.s.selection.clear();
    app.status = None;
    app.toggle_visibility();
    assert_eq!(app.hint.as_deref(), Some("Show/Hide: select bodies, faces or edges first"));
  }

  #[test]
  fn root_labels_follow_fusion() {
    let mut app = App::with_session(Session::default());
    assert_eq!(root_label(&app), "(Unsaved)");
    assert_eq!(crate::documents::tab_label(&app.s), "Untitled");
    let user = user_name();
    assert_eq!(window_title(&app), format!("Untitled ({user}) - FissionOxide"));
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    assert!(app.s.dirty);
    assert_eq!(window_title(&app), format!("Untitled* ({user}) - FissionOxide"));
    assert_eq!(crate::documents::tab_label(&app.s), "Untitled", "the tab shows the lock, not a star, until saved");
    // saved to a file: the name and version show, the tab stars its unsaved changes
    app.s.path = Some(std::path::PathBuf::from("/tmp/Bracket.fox"));
    assert_eq!(root_label(&app), "Bracket v1");
    assert_eq!(crate::documents::tab_label(&app.s), "Bracket v1*");
    assert_eq!(window_title(&app), format!("Bracket v1* ({user}) - FissionOxide"));
    app.s.dirty = false;
    assert_eq!(crate::documents::tab_label(&app.s), "Bracket v1");
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
    let items = root_menu(&app);
    let cloud: Vec<&str> =
      items.iter().flatten().filter(|i| i.cmd == RowCmd::NotYet).map(|i| i.label.as_str()).collect();
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
    assert!(
      app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Appearance),
      "{:?}",
      app.dialog.as_ref().map(|d| d.kind)
    );
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

  /// A flat surface body (a Patch of a sketch rectangle), with a box beside it.
  fn patched() -> (App, BodyId, BodyId) {
    let mut app = boxed();
    let solid = app.s.model.bodies[0].id;
    app.create_sketch(json!("xy"));
    app.run("sketch.rect", json!({"corner1": [20, 0], "corner2": [30, 10]}));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("surface.patch", json!({"sketch": sk.0}));
    let surface = app.s.model.bodies.last().unwrap().id;
    assert_ne!(surface, solid, "the patch made a body");
    (app, surface, solid)
  }

  #[test]
  fn a_surface_body_row_has_fusions_icon_and_menu() {
    let (mut app, surface, solid) = patched();
    assert_eq!(super::super::body_icon(&app, surface), Icon::SurfaceBody);
    assert_eq!(super::super::body_icon(&app, solid), Icon::Body);
    let items = surface_body_menu(&app, surface);
    assert_eq!(
      labels(&items),
      [
        "Move/Copy",
        "Move to Group",
        "Create Selection Set",
        "Configure",
        "Physical Material",
        "Appearance",
        "Texture Map Controls",
        "Properties",
        "Save As Mesh",
        "-",
        "Copy",
        "Cut",
        "Delete",
        "Remove",
        "Rename",
        "-",
        "Display Detail Control",
        "-",
        "Show/Hide",
        "Selectable/Unselectable",
        "-",
        "Opacity Control",
        "Isolate",
        "-",
        "Find in Window",
      ]
    );
    let keys: Vec<(&str, &str)> = items.iter().flatten().filter_map(|i| i.key.map(|k| (i.label.as_str(), k))).collect();
    assert_eq!(
      keys,
      [
        ("Move/Copy", "M"),
        ("Appearance", "A"),
        ("Copy", "Ctrl+C"),
        ("Cut", "Ctrl+X"),
        ("Delete", "Del"),
        ("Show/Hide", "V")
      ]
    );
    let greyed: Vec<&str> =
      items.iter().flatten().filter(|i| i.cmd == RowCmd::NotYet).map(|i| i.label.as_str()).collect();
    assert_eq!(
      greyed,
      ["Move to Group", "Create Selection Set", "Texture Map Controls", "Cut", "Display Detail Control"]
    );

    let ctx = egui::Context::default();
    // Physical Material is enabled: the body outside a component takes its own
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::PhysicalMaterial);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Material));
    assert_eq!(app.s.selection, [Sel::Body { body: surface }]);
    app.dialog = None;
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::MoveCopy);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Move));
    app.dialog = None;
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::Copy);
    assert_eq!(app.clipboard, Some(crate::clipboard::Clip::Bodies(vec![surface])));
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::Isolate);
    assert_eq!(app.ctx_menu.isolate, Some(vec![surface]));
    assert!(labels(&surface_body_menu(&app, surface)).contains(&"Unisolate".into()));
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::Isolate);
    assert!(app.ctx_menu.isolate.is_none());
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::Selectable);
    assert_eq!(app.ctx_menu.unselectable, vec![surface]);
    assert!(labels(&surface_body_menu(&app, surface)).contains(&"Selectable".into()));
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::Selectable);
    assert!(app.ctx_menu.unselectable.is_empty());
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::ShowHide);
    assert!(!app.s.body_visible(surface));
    run_surface_body_cmd(&mut app, &ctx, surface, RowCmd::Remove);
    assert!(app.s.model.body(surface).is_none(), "removed");
    assert!(app.s.model.body(solid).is_some());
  }

  #[test]
  fn origin_folder_starts_hidden_and_its_menu_is_fusions() {
    let mut app = App::with_session(Session::default());
    assert!(!app.origin_shown, "Origin starts hidden, as in Fusion");
    assert_eq!(
      labels(&origin_folder_menu(&app)),
      ["Create Selection Set", "-", "Show/Hide", "Show All", "Show/Hide Planes", "Show/Hide Axes", "Find in Window"]
    );
    // the children's eyes start on but greyed: a child shows only with the folder's eye too
    assert!(origin::planes_shown(&app) && origin::axes_shown(&app));
    assert!(OriginItem::ALL.iter().all(|i| !origin::visible(&app, *i)));
    origin_show_hide(&mut app);
    assert!(app.origin_shown && OriginItem::ALL.iter().all(|i| origin::visible(&app, *i)));
    origin_show_hide(&mut app);
    assert!(!app.origin_shown && !origin::visible(&app, OriginItem::Xy));
    // Show/Hide Planes and Show/Hide Axes toggle the children's own eyes, not the folder's
    run_origin_folder_cmd(&mut app, RowCmd::ShowHidePlanes);
    assert!(!origin::planes_shown(&app) && !app.origin_shown && origin::axes_shown(&app));
    assert_eq!(labels(&origin_folder_menu(&app))[4], "Show/Hide Planes");
    run_origin_folder_cmd(&mut app, RowCmd::ShowHidePlanes);
    assert!(origin::planes_shown(&app));
    run_origin_folder_cmd(&mut app, RowCmd::ShowHideAxes);
    assert!(!origin::axes_shown(&app) && !origin::eye(&app, OriginItem::Y) && origin::eye(&app, OriginItem::Point));
    origin_show_hide(&mut app);
    assert!(origin::visible(&app, OriginItem::Xy) && !origin::visible(&app, OriginItem::Y));
    run_origin_folder_cmd(&mut app, RowCmd::ShowHidePlanes);
    assert!(app.origin_shown && !origin::planes_shown(&app), "Show/Hide Planes leaves the folder shown");
    // the point's and axes' own menus are their eyes
    assert_eq!(labels(&origin_item_menu(&app, OriginItem::Z)), ["Show/Hide"]);
    run_origin_item_cmd(&mut app, OriginItem::Z, RowCmd::ShowHide);
    assert!(origin::eye(&app, OriginItem::Z) && !origin::eye(&app, OriginItem::X));
    app.objects.origin_planes = false;
    app.objects.origin_points = false;
    run_origin_folder_cmd(&mut app, RowCmd::ShowAll);
    assert!(
      app.origin_shown
        && app.objects.origin_planes
        && app.objects.origin_points
        && OriginItem::ALL.iter().all(|i| origin::visible(&app, *i))
    );
    run_origin_folder_cmd(&mut app, RowCmd::FindInWindow);
    assert!(app.status.as_ref().is_none_or(|(_, err)| !err), "{:?}", app.status);
  }

  #[test]
  fn units_and_part_design_menus_open_their_dialogs() {
    let mut app = App::with_session(Session::default());
    assert_eq!(labels(&units_menu()), ["Change Active Units"]);
    run_units_cmd(&mut app, RowCmd::ChangeUnits);
    assert!(crate::doc_dialogs::units_open(&app));
    assert_eq!(labels(&design_type_menu()), ["Convert Design Type"]);
    run_design_type_cmd(&mut app, RowCmd::ConvertDesignType);
    assert!(crate::doc_dialogs::convert_open(&app));
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
