//! The browser's Sketches folder and sketch rows, as in Fusion: the folder's eye and right-click
//! menu, an eye on every sketch row, the sketch row's menu (with its shortcut hints), the canvas
//! highlight while a row is hovered (no tooltip, no status text), and the per-sketch Hide
//! Projected / Hide Construction Geometries.

use egui::RectAlign;
use fox_engine::Sel;
use fox_engine::model::{FeatureId, FeatureKind, OriginPlane, PlaneRef};
use serde_json::{Value, json};

use super::{RowCmd, RowItem, browser_edit, is_open, reveal_row, row_item, row_key, row_menu, tree_row};
use crate::dialogs::Kind;
use crate::icons::Icon;
use crate::{App, RenameTarget, marking, theme};

/// What the browser's sketch rows hide in the canvas (a view state; nothing in the design).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SketchRowState {
  /// The Sketches folder's eye is off: no sketch is drawn, each keeping its own eye.
  pub(crate) folder_hidden: bool,
  /// Sketches drawn without their projected curves (Hide Projected Geometries).
  pub(crate) hidden_projected: Vec<FeatureId>,
  /// Sketches drawn without their construction curves (Hide Construction Geometries).
  pub(crate) hidden_construction: Vec<FeatureId>,
}

/// Whether the canvas draws a sketch: its own eye, under the Sketches folder's.
pub(crate) fn drawn(app: &App, fid: FeatureId) -> bool {
  (!app.sketch_rows.folder_hidden || app.s.edit_sketch == Some(fid)) && fox_engine::scene::sketch_visible(&app.s, fid)
}

/// The sketches the browser lists (a sketch made to place a single hole isn't one of the user's:
/// Fusion shows none).
fn listed(app: &App) -> Vec<(FeatureId, String)> {
  let placed = crate::dialogs::placement_sketches(&app.s);
  app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Sketch { .. }) && !placed.contains(&f.id))
    .map(|f| (f.id, f.name.clone()))
    .collect()
}

/// The plane a sketch sits on, as a dialog takes it, with its name: an origin plane, a
/// construction plane or a planar face (`None` for a free-standing plane).
fn sketch_plane(app: &App, fid: FeatureId) -> Option<(Value, String)> {
  let FeatureKind::Sketch { plane, .. } = &app.s.design.feature(fid)?.kind else { return None };
  match plane {
    PlaneRef::Origin { plane } => {
      let n = match plane {
        OriginPlane::Xy => "xy",
        OriginPlane::Xz => "xz",
        OriginPlane::Yz => "yz",
      };
      Some((json!(n), format!("{} plane", n.to_uppercase())))
    }
    PlaneRef::Construction { feature } => {
      Some((json!({"feature": feature.0}), app.s.design.feature(*feature)?.name.clone()))
    }
    PlaneRef::Face { face, .. } => {
      let id = app.s.model.body(face.body)?.find_face(face)?;
      Some((json!({"body": face.body.0, "face": id}), format!("Face of {}", app.s.body_name(face.body))))
    }
    _ => None,
  }
}

/// A sketch row's menu, as Fusion's (`None` is a separator). Move to Group, Create Selection Set
/// and Slice Sketch are listed greyed; Slice Sketch is missing while the
/// sketch is hidden, as in Fusion.
pub(crate) fn sketch_menu(app: &App, fid: FeatureId) -> Vec<Option<RowItem>> {
  let profiles = !app.hidden_profiles.contains(&fid);
  let dims = app.shown_dims.contains(&fid);
  let visible = fox_engine::scene::sketch_visible(&app.s, fid);
  let projected = !app.sketch_rows.hidden_projected.contains(&fid);
  let construction = !app.sketch_rows.hidden_construction.contains(&fid);
  let on_plane = |cmd| if sketch_plane(app, fid).is_some() { cmd } else { RowCmd::NotYet };
  let configure = if app.s.design.configurations.list.is_empty() { RowCmd::NotYet } else { RowCmd::Configure };
  let mut items = vec![
    row_item(Icon::Folder, "Move to Group", RowCmd::NotYet),
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    row_key(Icon::Extrude, "Extrude", "E", RowCmd::Extrude),
    row_item(Icon::Plane, "Offset Plane", on_plane(RowCmd::OffsetPlane)),
    None,
    row_item(Icon::Sketch, "Edit Sketch", RowCmd::EditSketch),
    row_item(Icon::Plane, "Redefine Sketch Plane", RowCmd::RedefinePlane),
    row_item(Icon::Select, "Select Sketch Plane", on_plane(RowCmd::SelectSketchPlane)),
  ];
  if visible {
    items.push(row_item(Icon::Section, "Slice Sketch", RowCmd::NotYet));
  }
  items.extend([
    row_item(Icon::ConfigTable, "Configure", configure),
    None,
    row_item(Icon::ExportDxf, "Export DXF", RowCmd::SaveAsDxf),
    row_key(Icon::Remove, "Delete", "Del", RowCmd::Delete),
    row_item(Icon::Params, "Rename", RowCmd::Rename),
    None,
    row_item(Icon::LookAt, "Look At", RowCmd::LookAt),
    None,
    row_item(Icon::SketchSmall, if profiles { "Hide Profile" } else { "Show Profile" }, RowCmd::ProfileVisibility),
    row_item(Icon::Dimension, if dims { "Hide Dimension" } else { "Show Dimension" }, RowCmd::DimensionVisibility),
    row_item(
      Icon::Project,
      if projected { "Hide Projected Geometries" } else { "Show Projected Geometries" },
      RowCmd::ProjectedVisibility,
    ),
    row_item(
      Icon::Construction,
      if construction { "Hide Construction Geometries" } else { "Show Construction Geometries" },
      RowCmd::ConstructionVisibility,
    ),
    row_key(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", "V", RowCmd::ShowHide),
    None,
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
    row_item(Icon::History, "Find in Timeline", RowCmd::FindInTimeline),
  ]);
  items
}

/// Whether a sketch sits on a plane Select Sketch Plane can pick.
pub(crate) fn has_plane(app: &App, fid: FeatureId) -> bool {
  sketch_plane(app, fid).is_some()
}

/// Extrude from a sketch's menu (a browser row or timeline items): every closed profile of the
/// sketches picked, as Fusion's.
pub(crate) fn extrude_sketches(app: &mut App, sketches: &[FeatureId]) {
  app.s.selection = sketches
    .iter()
    .flat_map(|f| {
      let n = app.s.model.sketches.get(f).map_or(0, |r| r.profiles.len());
      (0..n).map(move |index| Sel::Profile { sketch: *f, index })
    })
    .collect();
  app.start(Kind::Extrude);
}

/// Runs a sketch row's menu command.
pub(crate) fn run_sketch_cmd(app: &mut App, fid: FeatureId, name: &str, cmd: RowCmd) {
  let toggle = |v: &mut Vec<FeatureId>| {
    if let Some(i) = v.iter().position(|f| *f == fid) {
      v.remove(i);
    } else {
      v.push(fid);
    }
  };
  match cmd {
    RowCmd::Extrude => extrude_sketches(app, &[fid]),
    RowCmd::OffsetPlane => {
      let Some((plane, label)) = sketch_plane(app, fid) else { return };
      app.s.selection.clear();
      app.start(Kind::OffsetPlane);
      if let Some(d) = app.dialog.as_mut() {
        d.use_plane(&mut app.s, plane, label);
      }
    }
    RowCmd::EditSketch => app.edit_sketch(fid),
    RowCmd::SelectSketchPlane => select_sketch_plane(app, fid),
    RowCmd::Configure => {
      // the sketch's suppression becomes a column of the configuration table
      let Some(first) = app.s.design.configurations.list.first().map(|c| c.name.clone()) else { return };
      let suppressed = app.s.design.feature(fid).is_some_and(|f| f.suppressed);
      app.run("config.set", json!({"configuration": first, "feature": fid.0, "suppressed": suppressed}));
      crate::config_ui::open_table();
    }
    RowCmd::ProfileVisibility => toggle(&mut app.hidden_profiles),
    RowCmd::DimensionVisibility => toggle(&mut app.shown_dims),
    RowCmd::ProjectedVisibility => toggle(&mut app.sketch_rows.hidden_projected),
    RowCmd::ConstructionVisibility => toggle(&mut app.sketch_rows.hidden_construction),
    RowCmd::LookAt => {
      if let Some(plane) = app.s.model.sketches.get(&fid).map(|r| r.plane) {
        app.look_at_plane(&plane);
      }
    }
    // Fusion's EXPORT DXF dialog (unit, visible geometry), whose OK asks where to save
    RowCmd::SaveAsDxf => {
      app.start(crate::dialogs::Kind::ExportDxf);
      if let Some(d) = app.dialog.as_mut() {
        d.dxf_begin(&app.s, fid);
      }
    }
    RowCmd::RedefinePlane => {
      app.start(crate::dialogs::Kind::RedefinePlane);
      if let Some(d) = app.dialog.as_mut() {
        d.redefine_begin(&mut app.s, fid);
      }
    }
    RowCmd::FindInWindow => marking::find_in_window(app, &[], Some(fid)),
    // the sketch is selected, which boxes its timeline icon
    RowCmd::FindInTimeline => app.s.selection = vec![Sel::Feature { feature: fid }],
    RowCmd::ShowHide => {
      let visible = fox_engine::scene::sketch_visible(&app.s, fid);
      app.run("sketch.visible", json!({"sketch": fid.0, "visible": !visible}));
    }
    RowCmd::Rename => app.rename = Some((RenameTarget::Feature(fid.0), name.to_string())),
    RowCmd::Delete => {
      app.run("feature.delete", json!({"feature": fid.0}));
    }
    _ => {}
  }
}

/// Select Sketch Plane: selects the face or plane the sketch sits on (a face reads out its area
/// in the status bar, as in Fusion).
fn select_sketch_plane(app: &mut App, fid: FeatureId) {
  let Some((plane, _)) = sketch_plane(app, fid) else { return };
  if let (Some(body), Some(face)) =
    (plane.get("body").and_then(Value::as_u64), plane.get("face").and_then(Value::as_u64))
  {
    crate::origin::clear(app);
    app.s.selection = vec![Sel::Face { body: fox_engine::model::BodyId(body as u32), face: face as u32 }];
    return;
  }
  // an origin or construction plane is picked as the browser picks it
  app.s.selection.clear();
  crate::origin::clear(app);
  app.picked_plane = Some(plane);
}

/// The Sketches folder's menu, as Fusion's (New Sketch Group and Create Selection Set greyed).
pub(crate) fn sketches_folder_menu(app: &App) -> Vec<Option<RowItem>> {
  let shown = !app.sketch_rows.folder_hidden;
  vec![
    row_item(Icon::Folder, "New Sketch Group", RowCmd::NotYet),
    row_item(Icon::Select, "Create Selection Set", RowCmd::NotYet),
    None,
    row_key(if shown { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", "V", RowCmd::ShowHide),
    row_item(Icon::Eye, "Show All", RowCmd::ShowAll),
  ]
}

/// Runs a Sketches folder menu command: Show/Hide is the folder's eye; Show All turns it and
/// every sketch's eye on.
pub(crate) fn run_sketches_folder_cmd(app: &mut App, cmd: RowCmd) {
  match cmd {
    RowCmd::ShowHide => app.sketch_rows.folder_hidden = !app.sketch_rows.folder_hidden,
    RowCmd::ShowAll => {
      app.sketch_rows.folder_hidden = false;
      for (fid, _) in listed(app) {
        if !fox_engine::scene::sketch_visible(&app.s, fid) {
          app.run("sketch.visible", json!({"sketch": fid.0, "visible": true}));
        }
      }
    }
    _ => {}
  }
}

/// A browser row's right-click menu, opening at the pointer: downward, or above with its bottom
/// edge at the pointer when there is no room below, as Fusion's.
fn context_menu(r: &egui::Response, add: impl FnOnce(&mut egui::Ui)) {
  egui::Popup::context_menu(r).align(RectAlign::BOTTOM_START).align_alternatives(&[RectAlign::TOP_START]).show(add);
}

/// The Sketches folder and its rows.
pub(super) fn sketches_section(app: &mut App, ui: &mut egui::Ui) {
  let sketches = listed(app);
  if sketches.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-sketches");
  // the folder starts collapsed, opens on the sketch being edited and closes on Finish Sketch
  if let Some(open) = app.browser_sketches.take() {
    ui.data_mut(|d| d.insert_temp(id.with("open"), open));
  }
  let shown = !app.sketch_rows.folder_hidden;
  let some_hidden = sketches.iter().any(|(fid, _)| !fox_engine::scene::sketch_visible(&app.s, *fid));
  let eye = super::folder_eye(ui, shown, some_hidden);
  let ev = tree_row(ui, id, 1, Some(false), eye, Icon::Folder, "Sketches", false, theme::TEXT);
  if ev.eye {
    app.sketch_rows.folder_hidden = shown;
  }
  if let Some(r) = &ev.response {
    if r.hovered() {
      marking::peek(app, Vec::new(), "Sketches".into());
    }
    context_menu(r, |ui| {
      let items = sketches_folder_menu(app);
      if let Some(cmd) = row_menu(ui, &items) {
        run_sketches_folder_cmd(app, cmd);
      }
    });
  }
  if !is_open(ui, id, false) {
    return;
  }
  for (fid, name) in sketches {
    let editing = app.s.edit_sketch == Some(fid);
    let selected = editing || app.s.selection.contains(&Sel::Feature { feature: fid });
    let st = app.s.model.status(fid).cloned().unwrap_or_default();
    let color = if st.error.is_some() {
      theme::ERROR
    } else if st.warning.is_some() {
      theme::WARNING
    } else if st.rolled_back {
      theme::MUTED
    } else {
      theme::TEXT
    };
    let row = id.with(fid.0);
    let visible = fox_engine::scene::sketch_visible(&app.s, fid);
    let ev = tree_row(ui, row, 2, None, Some(visible), Icon::SketchSmall, &name, selected, color);
    let target = RenameTarget::Feature(fid.0);
    if ev.eye {
      // the eye hides or shows the sketch and leaves the selection alone, as in Fusion
      app.run("sketch.visible", json!({"sketch": fid.0, "visible": !visible}));
    } else {
      // double-click edits the sketch, as in Fusion; a slow click renames it
      browser_edit::slow_click(ui, &ev, row, target, &name, selected && !editing);
      if ev.double_clicked {
        app.edit_sketch(fid);
      } else if ev.clicked {
        app.s.selection = vec![Sel::Feature { feature: fid }];
      }
      browser_edit::slow_click_due(ui, row, app.s.selection.contains(&Sel::Feature { feature: fid }));
    }
    browser_edit::edit_box(app, ui, &ev, row, 2);
    if let Some(r) = ev.response {
      reveal_row(app, Sel::Feature { feature: fid }, &r);
      // hovering lights the sketch in the canvas and names it nowhere: no tooltip, no status
      if r.hovered() {
        marking::peek_sketch(app, fid);
      }
      context_menu(&r, |ui| {
        let items = sketch_menu(app, fid);
        match row_menu(ui, &items) {
          // renamed in place in the browser row
          Some(RowCmd::Rename) => browser_edit::begin(ui, row, target, &name),
          Some(cmd) => run_sketch_cmd(app, fid, &name, cmd),
          None => {}
        }
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use fox_engine::scene::{Pick, SceneBuilder, ViewOptions};

  use super::*;

  fn labels(items: &[Option<RowItem>]) -> Vec<String> {
    items.iter().map(|i| i.as_ref().map(|i| i.label.clone()).unwrap_or_else(|| "-".into())).collect()
  }

  fn keys(items: &[Option<RowItem>]) -> Vec<(String, &'static str)> {
    items.iter().flatten().filter_map(|i| i.key.map(|k| (i.label.clone(), k))).collect()
  }

  /// Sketch1 (a rectangle on XY), finished.
  fn sketched() -> (App, FeatureId) {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
    app.finish_sketch();
    (app, sk)
  }

  #[test]
  fn sketch_row_menu_is_fusions() {
    let (mut app, sk) = sketched();
    let items = sketch_menu(&app, sk);
    assert_eq!(
      labels(&items),
      [
        "Move to Group",
        "Create Selection Set",
        "Extrude",
        "Offset Plane",
        "-",
        "Edit Sketch",
        "Redefine Sketch Plane",
        "Select Sketch Plane",
        "Slice Sketch",
        "Configure",
        "-",
        "Export DXF",
        "Delete",
        "Rename",
        "-",
        "Look At",
        "-",
        "Hide Profile",
        "Show Dimension",
        "Hide Projected Geometries",
        "Hide Construction Geometries",
        "Show/Hide",
        "-",
        "Find in Window",
        "Find in Timeline",
      ]
    );
    assert_eq!(keys(&items), [("Extrude".into(), "E"), ("Delete".into(), "Del"), ("Show/Hide".into(), "V")]);
    let greyed: Vec<String> =
      items.iter().flatten().filter(|i| i.cmd == RowCmd::NotYet).map(|i| i.label.clone()).collect();
    assert_eq!(greyed, ["Move to Group", "Create Selection Set", "Slice Sketch", "Configure"]);
    // the toggles read the other way once used
    for cmd in [
      RowCmd::ProfileVisibility,
      RowCmd::DimensionVisibility,
      RowCmd::ProjectedVisibility,
      RowCmd::ConstructionVisibility,
    ] {
      run_sketch_cmd(&mut app, sk, "Sketch1", cmd);
    }
    assert_eq!(app.hidden_profiles, vec![sk]);
    assert_eq!(app.shown_dims, vec![sk]);
    assert_eq!(app.sketch_rows.hidden_projected, vec![sk]);
    assert_eq!(app.sketch_rows.hidden_construction, vec![sk]);
    let l = labels(&sketch_menu(&app, sk));
    for want in ["Show Profile", "Hide Dimension", "Show Projected Geometries", "Show Construction Geometries"] {
      assert!(l.contains(&want.into()), "{want} in {l:?}");
    }
    // Show/Hide toggles the sketch; hidden, Slice Sketch leaves the menu
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ShowHide);
    assert!(!fox_engine::scene::sketch_visible(&app.s, sk));
    assert!(!labels(&sketch_menu(&app, sk)).contains(&"Slice Sketch".into()));
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ShowHide);
    assert!(fox_engine::scene::sketch_visible(&app.s, sk));
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::FindInWindow);
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::LookAt);
    assert!(app.status.as_ref().is_none_or(|(_, err)| !err), "{:?}", app.status);
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::FindInTimeline);
    assert_eq!(app.s.selection, vec![Sel::Feature { feature: sk }]);
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::SelectSketchPlane);
    assert_eq!(app.picked_plane, Some(json!("xy")), "the XY plane is picked");
    assert!(app.s.selection.is_empty());
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::EditSketch);
    assert_eq!(app.s.edit_sketch, Some(sk));
  }

  #[test]
  fn sketch_row_extrude_and_offset_plane() {
    let (mut app, sk) = sketched();
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::Extrude);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Extrude));
    assert_eq!(app.s.selection, vec![Sel::Profile { sketch: sk, index: 0 }], "the sketch's profile is picked");
    app.cancel_dialog();
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::OffsetPlane);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::OffsetPlane));
  }

  #[test]
  fn select_sketch_plane_selects_the_face() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let body = app.s.model.bodies[0].id;
    let top = app.s.model.body(body).unwrap().faces().iter().find(|f| f.normal.z > 0.9).expect("a top face").id;
    app.create_sketch(json!({"body": body.0, "face": top}));
    let sk = app.s.edit_sketch.expect("sketching on the face");
    app.finish_sketch();
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::SelectSketchPlane);
    assert_eq!(app.s.selection, vec![Sel::Face { body, face: top }]);
  }

  #[test]
  fn sketches_folder_menu_is_fusions() {
    let (mut app, sk) = sketched();
    let items = sketches_folder_menu(&app);
    assert_eq!(labels(&items), ["New Sketch Group", "Create Selection Set", "-", "Show/Hide", "Show All"]);
    assert_eq!(keys(&items), [("Show/Hide".into(), "V")]);
    assert!(drawn(&app, sk));
    run_sketches_folder_cmd(&mut app, RowCmd::ShowHide);
    assert!(app.sketch_rows.folder_hidden && !drawn(&app, sk), "the folder's eye hides every sketch");
    assert!(fox_engine::scene::sketch_visible(&app.s, sk), "the sketch keeps its own eye");
    let opts = crate::viewport::scene_options(&mut app);
    assert!(opts.hide_sketches);
    // Show All turns the folder and every sketch on
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ShowHide);
    run_sketches_folder_cmd(&mut app, RowCmd::ShowAll);
    assert!(!app.sketch_rows.folder_hidden && drawn(&app, sk));
  }

  /// Finish Sketch on a sketch a feature uses leaves it hidden, even after it was shown.
  #[test]
  fn consumed_sketch_hidden_after_finish() {
    let (mut app, sk) = sketched();
    app.run("extrude", json!({"sketch": sk.0, "distance": 5}));
    assert!(!fox_engine::scene::sketch_visible(&app.s, sk), "a used sketch hides");
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ShowHide);
    assert!(fox_engine::scene::sketch_visible(&app.s, sk), "its eye shows it");
    app.edit_sketch(sk);
    app.finish_sketch();
    assert!(!fox_engine::scene::sketch_visible(&app.s, sk), "Finish Sketch hides it again");
  }

  /// A selected sketch fills its profiles in the selection blue with light curves; a hovered one
  /// (peeked) fills them a translucent light blue.
  #[test]
  fn sketch_selected_and_peeked_in_the_canvas() {
    let (mut app, sk) = sketched();
    let mut b = SceneBuilder::default();
    let fill = |app: &App, b: &mut SceneBuilder, opts: &ViewOptions| {
      let (sc, picks) = b.build(&app.s, opts);
      let pid = picks.id_of(&Pick::Profile { sketch: sk, index: 0 }).expect("the profile is drawn");
      sc.meshes.iter().find(|m| m.tri_ids.first() == Some(&pid)).expect("the profile is shaded").color
    };
    assert_eq!(fill(&app, &mut b, &ViewOptions::default()), fox_engine::scene::PROFILE_COLOR);
    let peek = ViewOptions { peek_sketches: vec![sk], ..Default::default() };
    assert_eq!(fill(&app, &mut b, &peek), fox_engine::scene::PROFILE_PEEK);
    app.s.selection = vec![Sel::Feature { feature: sk }];
    assert_eq!(fill(&app, &mut b, &ViewOptions::default()), fox_engine::scene::PROFILE_SKETCH_SELECTED);
    let (sc, _) = b.build(&app.s, &ViewOptions::default());
    assert!(
      sc.lines.iter().any(|l| l.color == fox_engine::scene::SKETCH_CURVE_ON_SELECTED),
      "the curves draw light over the fill"
    );
  }

  /// Hovering a sketch row lights the sketch and puts nothing in the status bar.
  #[test]
  fn sketch_row_hover_peeks_without_a_label() {
    let (mut app, sk) = sketched();
    marking::peek_sketch(&mut app, sk);
    let ctx = egui::Context::default();
    marking::end_frame(&mut app, &ctx);
    assert_eq!(app.ctx_menu.peek_sketches, vec![sk]);
    assert_eq!(app.ctx_menu.peek_label, None);
  }
}
