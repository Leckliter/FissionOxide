//! Timeline groups as in Fusion: select adjacent timeline items (Shift+click a range, Ctrl+click
//! to add one), right-click, Create Group. A group shows as one folder icon; clicking it expands
//! it (its features, bracketed, behind an open folder that collapses it again). Its menu renames
//! it, suppresses or deletes all its features, rolls the marker past it or ungroups it. A feature
//! next to a group can Move to Group.

use egui::{Color32, Rect, Stroke, Vec2, vec2};
use fox_engine::Sel;
use fox_engine::model::FeatureId;
use serde_json::json;

use super::menu_item;
use crate::icons::{self, Icon};
use crate::{App, RenameTarget, theme};

/// A group as the timeline draws it: its id, name and the timeline positions of its features
/// (only groups whose features sit side by side collapse).
#[derive(Clone, Debug)]
pub(super) struct Shown {
  pub id: u32,
  pub name: String,
  pub first: usize,
  pub last: usize,
}

/// The groups the timeline can show collapsed, by their first feature's position.
pub(super) fn shown(app: &App) -> Vec<Shown> {
  let d = &app.s.design;
  d.timeline_groups
    .iter()
    .filter_map(|g| {
      let mut at: Vec<usize> = g.features.iter().filter_map(|f| d.index_of(*f)).collect();
      at.sort_unstable();
      let (first, last) = (*at.first()?, *at.last()?);
      (last - first + 1 == at.len()).then(|| Shown { id: g.id, name: g.name.clone(), first, last })
    })
    .collect()
}

fn open_id(group: u32) -> egui::Id {
  egui::Id::new(("timeline-group-open", group))
}

/// Whether a group is expanded (a view state, like Fusion's: not saved, no undo step).
pub(super) fn is_open(ui: &egui::Ui, group: u32) -> bool {
  ui.data(|d| d.get_temp(open_id(group)).unwrap_or(false))
}

fn set_open(ctx: &egui::Context, group: u32, open: bool) {
  ctx.data_mut(|d| d.insert_temp(open_id(group), open));
}

fn members(app: &App, g: &Shown) -> Vec<FeatureId> {
  app.s.design.features[g.first..=g.last].iter().map(|f| f.id).collect()
}

/// A click on a timeline item: Shift selects the range from the last picked item, Ctrl adds or
/// removes the item, a plain click selects only it.
pub(crate) fn click_feature(app: &mut App, id: FeatureId, shift: bool, ctrl: bool) {
  let item = Sel::Feature { feature: id };
  let picked: Vec<FeatureId> = app
    .s
    .selection
    .iter()
    .filter_map(|s| match s {
      Sel::Feature { feature } => Some(*feature),
      _ => None,
    })
    .collect();
  let d = &app.s.design;
  if shift && let (Some(anchor), Some(to)) = (picked.last().and_then(|a| d.index_of(*a)), d.index_of(id)) {
    let (a, b) = (anchor.min(to), anchor.max(to));
    let mut sel: Vec<Sel> = d.features[a..=b].iter().map(|f| Sel::Feature { feature: f.id }).collect();
    // the anchor stays last, so the next Shift+click ranges from it
    if let Some(p) = sel.iter().position(|s| *s == Sel::Feature { feature: d.features[anchor].id }) {
      let anchor = sel.remove(p);
      sel.push(anchor);
    }
    app.s.selection = sel;
  } else if ctrl {
    app.s.selection.retain(|s| matches!(s, Sel::Feature { .. }));
    if let Some(p) = app.s.selection.iter().position(|s| *s == item) {
      app.s.selection.remove(p);
    } else {
      app.s.selection.push(item);
    }
  } else {
    app.s.selection = vec![item];
  }
}

/// What Create Group on the item `id` groups: the picked timeline items when `id` is one of them,
/// else `id` alone; `None` when they can't be grouped (not side by side, or grouped already).
pub(crate) fn create_group_targets(app: &App, id: FeatureId) -> Option<Vec<FeatureId>> {
  let picked: Vec<FeatureId> = app
    .s
    .selection
    .iter()
    .filter_map(|s| match s {
      Sel::Feature { feature } => Some(*feature),
      _ => None,
    })
    .collect();
  let targets = if picked.contains(&id) { picked } else { vec![id] };
  app.s.design.check_group(&targets).ok().map(|_| targets)
}

/// The group the item at timeline position `i` can Move to: one that ends just before it or
/// starts just after it (the item in no group itself).
pub(crate) fn move_to_group_target(app: &App, i: usize) -> Option<(u32, String)> {
  let d = &app.s.design;
  let id = d.features.get(i)?.id;
  if d.group_of(id).is_some() {
    return None;
  }
  shown(app).into_iter().find(|g| g.last + 1 == i || g.first == i + 1).map(|g| (g.id, g.name))
}

/// The group items in a feature's right-click menu: Create Group and Move to Group.
pub(super) fn feature_menu_items(app: &mut App, ui: &mut egui::Ui, i: usize, id: FeatureId) {
  let targets = create_group_targets(app, id);
  if ui.add_enabled_ui(targets.is_some(), |ui| menu_item(ui, Icon::Folder, "Create Group", None)).inner.clicked()
    && let Some(t) = targets
  {
    ui.close();
    if let Some(r) = app.run("timeline.group", json!({"features": t.iter().map(|f| f.0).collect::<Vec<_>>()}))
      && let Some(g) = r["group"].as_u64()
    {
      set_open(ui.ctx(), g as u32, false);
    }
  }
  if let Some((g, name)) = move_to_group_target(app, i)
    && menu_item(ui, Icon::Folder, &format!("Move to Group ({name})"), None).clicked()
  {
    ui.close();
    app.run("timeline.group_add", json!({"group": g, "features": [id.0]}));
  }
}

/// A group's icon: a closed folder for a collapsed group (click expands it), an open one at the
/// start of an expanded group (click collapses it). Returns its rect.
pub(super) fn group_icon(app: &mut App, ui: &mut egui::Ui, g: &Shown, open: bool, rolled_back: bool) -> Rect {
  let ids = members(app, g);
  let d = &app.s.design;
  let suppressed = ids.iter().all(|f| d.feature(*f).is_some_and(|f| f.suppressed));
  let failing = ids.iter().any(|f| app.s.model.status(*f).is_some_and(|s| s.error.is_some()));
  let selected = !open && ids.iter().all(|f| app.s.selection.contains(&Sel::Feature { feature: *f }));
  let size = if open { vec2(16.0, 28.0) } else { vec2(30.0, 28.0) };
  let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
  let p = ui.painter();
  if failing {
    p.rect(rect, 2.0, Color32::from_rgb(253, 225, 222), Stroke::new(1.0, theme::ERROR), egui::StrokeKind::Inside);
  }
  if selected {
    p.rect(rect, 2.0, theme::SELECTED, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
  } else if r.hovered() {
    p.rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let side = if open { 14.0 } else { 20.0 };
  let ir = Rect::from_center_size(rect.center(), Vec2::splat(side));
  if !open {
    // a second folder behind: several items in one
    icons::draw(p, ir.translate(vec2(3.0, -3.0)), Icon::Folder, Color32::from_gray(170));
  }
  if rolled_back || suppressed {
    icons::draw(p, ir, Icon::Folder, Color32::from_gray(185));
  } else {
    icons::draw_color(p, ir, Icon::Folder);
  }
  if suppressed {
    p.line_segment(
      [rect.left_bottom() + vec2(3.0, -3.0), rect.right_top() + vec2(-3.0, 3.0)],
      Stroke::new(1.0, theme::ERROR),
    );
  }
  let hint = format!("{}\n{} features — click to {}", g.name, ids.len(), if open { "collapse" } else { "expand" });
  let r = r.on_hover_text(hint);
  if r.clicked() {
    set_open(ui.ctx(), g.id, !open);
  }
  let (gid, name, last) = (g.id, g.name.clone(), g.last);
  r.context_menu(|ui| group_menu(app, ui, gid, &name, last, open, suppressed));
  rect
}

/// The line under an expanded group's features, from its open folder to its last feature.
pub(super) fn bracket(ui: &egui::Ui, from: Rect, to: Rect) {
  let y = from.bottom() - 1.0;
  let s = Stroke::new(2.0, theme::ACCENT.gamma_multiply(0.6));
  ui.painter().line_segment([egui::pos2(from.center().x, y), egui::pos2(to.right() - 2.0, y)], s);
  ui.painter().line_segment([egui::pos2(to.right() - 2.0, y), egui::pos2(to.right() - 2.0, y - 4.0)], s);
}

/// A group's right-click menu, as in Fusion.
fn group_menu(app: &mut App, ui: &mut egui::Ui, gid: u32, name: &str, last: usize, open: bool, suppressed: bool) {
  let label = if open { "Collapse Group" } else { "Expand Group" };
  if menu_item(ui, Icon::Folder, label, None).clicked() {
    ui.close();
    set_open(ui.ctx(), gid, !open);
  }
  ui.separator();
  if menu_item(ui, Icon::Remove, "Delete", Some("Del")).clicked() {
    ui.close();
    app.run("timeline.group_delete", json!({"group": gid}));
  }
  if menu_item(ui, Icon::Params, "Rename", None).clicked() {
    ui.close();
    app.rename = Some((RenameTarget::Group(gid), name.to_string()));
  }
  if menu_item(ui, Icon::History, "Roll Timeline Marker Here", None).clicked() {
    ui.close();
    app.run("timeline.marker", json!({"at": last + 1}));
  }
  let label = if suppressed { "Unsuppress Features" } else { "Suppress Features" };
  if menu_item(ui, Icon::EyeOff, label, None).clicked() {
    ui.close();
    app.run("timeline.group_suppress", json!({"group": gid, "suppressed": !suppressed}));
  }
  ui.separator();
  if menu_item(ui, Icon::Folder, "Ungroup", None).clicked() {
    ui.close();
    app.run("timeline.ungroup", json!({"group": gid}));
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use super::*;

  fn three() -> (App, Vec<FeatureId>) {
    let mut app = App::with_session(Session::default());
    for h in [10, 20, 30] {
      app.run("box", json!({"width": 10, "depth": 10, "height": h}));
    }
    let ids = app.s.design.features.iter().map(|f| f.id).collect();
    (app, ids)
  }

  #[test]
  fn shift_and_ctrl_click_pick_timeline_items() {
    let (mut app, f) = three();
    click_feature(&mut app, f[0], false, false);
    click_feature(&mut app, f[2], true, false);
    assert_eq!(app.s.selection.len(), 3, "Shift picks the range");
    click_feature(&mut app, f[1], false, true);
    assert_eq!(app.s.selection.len(), 2, "Ctrl takes one out");
    assert!(create_group_targets(&app, f[0]).is_none(), "f0 and f2 aren't side by side");
    click_feature(&mut app, f[1], false, false);
    click_feature(&mut app, f[2], false, true);
    assert_eq!(create_group_targets(&app, f[2]), Some(vec![f[1], f[2]]));
    // an item outside the pick groups on its own
    assert_eq!(create_group_targets(&app, f[0]), Some(vec![f[0]]));
  }

  #[test]
  fn create_group_then_move_to_group() {
    let (mut app, f) = three();
    let ids: Vec<u32> = f[1..].iter().map(|f| f.0).collect();
    app.run("timeline.group", json!({ "features": ids }));
    let g = shown(&app);
    assert_eq!((g.len(), g[0].first, g[0].last), (1, 1, 2));
    assert_eq!(g[0].name, "Group1");
    // the item before the group can join it; grouped items can't group again
    assert_eq!(move_to_group_target(&app, 0).map(|g| g.1), Some("Group1".to_string()));
    assert!(create_group_targets(&app, f[1]).is_none());
    app.run("timeline.group_add", json!({"group": g[0].id, "features": [f[0].0]}));
    assert_eq!(shown(&app)[0].first, 0);
    assert_eq!(app.s.undo_history()[..2], ["Move to Group", "Create Group"]);
  }
}
