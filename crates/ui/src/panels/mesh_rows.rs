//! A mesh body's right-click menu, as Fusion's: the browser row's, and the canvas list's (the same
//! entries plus Find in Browser). Rows FissionOxide can't run yet are listed greyed.

use fox_engine::Sel;
use fox_engine::model::BodyId;
use serde_json::json;

use super::{Kind, browser_edit, menu_item};
use crate::icons::Icon;
use crate::{App, RenameTarget, marking};

/// What a mesh body menu row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshCmd {
  MoveCopy,
  EditFeature,
  Configure,
  PhysicalMaterial,
  Appearance,
  Properties,
  SaveAsMesh,
  Copy,
  Delete,
  Remove,
  Rename,
  ShowHide,
  Selectable,
  Opacity,
  FindInBrowser,
  FindInWindow,
  FlatShaded,
  /// The canvas list's Opacity Control levels (percent).
  OpacityTo(u8),
  /// Listed, greyed, as Fusion has it but FissionOxide doesn't yet.
  NotYet,
}

/// One row of the menu.
#[derive(Debug, Clone)]
pub(crate) struct MeshItem {
  pub icon: Icon,
  pub label: String,
  /// The shortcut hint at the right of the row.
  pub key: Option<&'static str>,
  pub cmd: MeshCmd,
  pub enabled: bool,
}

fn row(icon: Icon, label: &str, key: Option<&'static str>, cmd: MeshCmd) -> Option<MeshItem> {
  Some(MeshItem { icon, label: label.into(), key, cmd, enabled: cmd != MeshCmd::NotYet })
}

fn greyed(icon: Icon, label: &str, key: Option<&'static str>) -> Option<MeshItem> {
  row(icon, label, key, MeshCmd::NotYet)
}

/// The mesh body menu (`None` is a separator); `canvas` adds Find in Browser, as the canvas list
/// has it.
pub(crate) fn mesh_body_menu(app: &App, body: BodyId, canvas: bool) -> Vec<Option<MeshItem>> {
  let flat = app.ctx_menu.flat_shaded.contains(&body);
  let visible = app.s.body_visible(body);
  let mut v = vec![
    row(Icon::Move, "Move/Copy", Some("M"), MeshCmd::MoveCopy),
    greyed(Icon::Folder, "Move to Group", None),
    greyed(Icon::Select, "Create Selection Set", None),
    None,
    row(Icon::Gear, "Edit Feature", None, MeshCmd::EditFeature),
    row(Icon::Configure, "Configure", None, MeshCmd::Configure),
    None,
    row(Icon::PhysMaterial, "Physical Material", None, MeshCmd::PhysicalMaterial),
    row(Icon::Appearance, "Appearance", Some("A"), MeshCmd::Appearance),
    greyed(Icon::Decal, "Texture Map Controls", None),
    row(Icon::Properties, "Properties", None, MeshCmd::Properties),
    None,
    row(Icon::ConvertMesh, "Save As Mesh", None, MeshCmd::SaveAsMesh),
    None,
    row(Icon::Body, "Copy", Some("Ctrl+C"), MeshCmd::Copy),
    greyed(Icon::Body, "Cut", Some("Ctrl+X")),
    row(Icon::Remove, "Delete", Some("Del"), MeshCmd::Delete),
    row(Icon::RemoveBody, "Remove", None, MeshCmd::Remove),
    row(Icon::Params, "Rename", None, MeshCmd::Rename),
    None,
    row(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", Some("V"), MeshCmd::ShowHide),
    None,
    row(Icon::Select, "Selectable/Unselectable", None, MeshCmd::Selectable),
    None,
    row(Icon::Body, "Opacity Control", None, MeshCmd::Opacity),
    None,
  ];
  if canvas {
    v.push(row(Icon::Folder, "Find in Browser", None, MeshCmd::FindInBrowser));
  }
  v.extend([
    row(Icon::Fit, "Find in Window", None, MeshCmd::FindInWindow),
    None,
    row(Icon::Display, if flat { "Flat Shaded  ✔" } else { "Flat Shaded" }, None, MeshCmd::FlatShaded),
    greyed(Icon::DirectEdit, "Direct Edit", None),
  ]);
  // physical materials belong to components in FissionOxide
  if app.s.design.component_of(body).is_none()
    && let Some(Some(m)) = v.iter_mut().find(|i| i.as_ref().is_some_and(|i| i.cmd == MeshCmd::PhysicalMaterial))
  {
    m.enabled = false;
  }
  v
}

/// Runs a mesh body menu command (Rename and Opacity Control need the row; see
/// [`mesh_context_menu`]).
pub(crate) fn run_mesh_cmd(app: &mut App, body: BodyId, cmd: MeshCmd) {
  let only = vec![Sel::Body { body }];
  match cmd {
    MeshCmd::MoveCopy => {
      app.s.selection = only;
      app.start(Kind::Move);
    }
    MeshCmd::EditFeature => {
      if let Some(f) = marking::body_feature(app, body) {
        super::edit_feature(app, f);
      }
    }
    MeshCmd::Configure => crate::config_ui::open_table(),
    MeshCmd::PhysicalMaterial => {
      app.s.selection = only;
      app.start(Kind::Material);
    }
    MeshCmd::Appearance => {
      app.s.selection = only;
      app.start(Kind::Form(&crate::forms::APPEARANCE));
    }
    MeshCmd::Properties => {
      app.s.selection = only;
      app.start(Kind::Inspect);
    }
    MeshCmd::SaveAsMesh => marking::save_as_mesh(app, body),
    MeshCmd::Copy => {
      // the browser and canvas menus copy through `crate::clipboard::copy`, which needs the
      // window; without one the body still goes on the clipboard
      app.s.selection = only;
      app.clipboard = crate::clipboard::clip_of(app);
    }
    MeshCmd::Delete => {
      app.s.selection = only;
      app.delete_selection();
    }
    MeshCmd::Remove => {
      app.run("body.remove", json!({"bodies": [body.0]}));
    }
    MeshCmd::ShowHide => marking::show_hide(app, &[body]),
    MeshCmd::Selectable => {
      let m = &mut app.ctx_menu;
      if m.unselectable.contains(&body) {
        m.unselectable.retain(|b| *b != body);
      } else {
        m.unselectable.push(body);
        app.s.selection.retain(|s| *s != Sel::Body { body });
      }
    }
    MeshCmd::FindInBrowser => marking::find_in_browser(app, Sel::Body { body }),
    MeshCmd::FindInWindow => marking::find_in_window(app, &[body], None),
    MeshCmd::FlatShaded => {
      let flat = &mut app.ctx_menu.flat_shaded;
      if flat.contains(&body) {
        flat.retain(|b| *b != body);
      } else {
        flat.push(body);
      }
    }
    MeshCmd::OpacityTo(pct) => {
      browser_edit::set_opacity(app, &[body], (pct as f32 * 2.55).round() as u8);
    }
    MeshCmd::Rename | MeshCmd::Opacity | MeshCmd::NotYet => {}
  }
}

/// The canvas list's Opacity Control levels, as its sub-list offers them.
pub(crate) const OPACITY_LEVELS: [u8; 5] = [100, 75, 50, 25, 0];

/// Draws a mesh body row's menu and runs what was clicked.
pub(super) fn mesh_context_menu(app: &mut App, ui: &mut egui::Ui, row: egui::Id, body: BodyId, name: &str) {
  let items = mesh_body_menu(app, body, false);
  let mut picked = None;
  for it in &items {
    let Some(it) = it else {
      ui.separator();
      continue;
    };
    match it.cmd {
      MeshCmd::Opacity => browser_edit::opacity_menu(app, ui, &[body]),
      _ if !it.enabled => {
        ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, it.key)).inner.on_disabled_hover_text(
          if it.cmd == MeshCmd::PhysicalMaterial {
            "Create a component from the body to give it a physical material"
          } else {
            "Not in FissionOxide yet"
          },
        );
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
    Some(MeshCmd::Rename) => browser_edit::begin(ui, row, RenameTarget::Body(body.0), name),
    Some(MeshCmd::Copy) => {
      app.s.selection = vec![Sel::Body { body }];
      crate::clipboard::copy(app, ui.ctx());
    }
    Some(cmd) => run_mesh_cmd(app, body, cmd),
    None => {}
  }
}
