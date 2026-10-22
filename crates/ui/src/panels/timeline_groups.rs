//! Timeline groups as in Fusion: select adjacent timeline items (Shift+click a range, Ctrl+click
//! to add one), right-click, Create Group. A collapsed group is one icon of three blue bars with
//! a "+" box under it that expands it; expanded, its items stand in the row with a bracket under
//! them whose "-" box collapses it again. Hovering a collapsed group lists its members; its menu
//! starts as a multi-selection's, expands it, deletes, renames or suppresses it. A feature next
//! to a group can Move to Group.

use egui::{Color32, Rect, Stroke, Vec2, vec2};
use fox_engine::Sel;
use fox_engine::model::FeatureId;
use serde_json::json;

use super::{TimelineEntry, menu_item};
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

/// Create Group (two or more picked items, greyed when they can't group) or Move to Group in a
/// feature's right-click menu.
pub(super) fn feature_menu_item(app: &mut App, ui: &mut egui::Ui, entry: TimelineEntry, i: usize, id: FeatureId) {
  if entry == TimelineEntry::CreateGroup {
    let targets = create_group_targets(app, id);
    if ui.add_enabled_ui(targets.is_some(), |ui| menu_item(ui, Icon::Folder, "Create Group", None)).inner.clicked()
      && let Some(t) = targets
    {
      ui.close();
      // the group collapses at once, named Group1, Group2, ... with no prompt (as Fusion's)
      if let Some(r) = app.run("timeline.group", json!({"features": t.iter().map(|f| f.0).collect::<Vec<_>>()}))
        && let Some(g) = r["group"].as_u64()
      {
        set_open(ui.ctx(), g as u32, false);
      }
    }
  } else if let Some((g, name)) = move_to_group_target(app, i)
    && menu_item(ui, Icon::Folder, &format!("Move to Group ({name})"), None).clicked()
  {
    ui.close();
    app.run("timeline.group_add", json!({"group": g, "features": [id.0]}));
  }
}

/// How many groups the picked timeline items cover whole: Fusion counts a picked group with its
/// members ("4 Features" for a group of three).
pub(crate) fn covered(app: &App) -> usize {
  let sel = &app.s.selection;
  app
    .s
    .design
    .timeline_groups
    .iter()
    .filter(|g| !g.features.is_empty() && g.features.iter().all(|f| sel.contains(&Sel::Feature { feature: *f })))
    .count()
}

/// Picks a group: its members, which Fusion's status bar counts with the group.
pub(crate) fn pick_group(app: &mut App, g: &Shown) {
  app.s.selection = members(app, g).into_iter().map(|feature| Sel::Feature { feature }).collect();
}

/// The lines of a collapsed group's tooltip: each member's icon, name and whether it is healthy
/// (computed, no error or warning, not suppressed: Fusion's green check).
pub(crate) fn tip_lines(app: &App, g: &Shown) -> Vec<(Icon, String, bool)> {
  app.s.design.features[g.first..=g.last]
    .iter()
    .map(|f| {
      let st = app.s.model.status(f.id);
      let ok = !f.suppressed && st.is_none_or(|s| s.error.is_none() && s.warning.is_none() && !s.rolled_back);
      (super::feature_icon(&f.kind), f.name.clone(), ok)
    })
    .collect()
}

/// A collapsed group's tooltip, as Fusion's: on the dark tooltip panel one line per member (its
/// icon, its name, a green check for a healthy one), a divider, then the group's name.
fn group_tip(app: &App, r: &egui::Response, g: &Shown) {
  let lines = tip_lines(app, g);
  let frame = egui::Frame::new()
    .fill(theme::TOOLTIP)
    .corner_radius(egui::CornerRadius::same(3))
    .inner_margin(egui::Margin::symmetric(8, 6))
    .shadow(r.ctx.global_style().visuals.popup_shadow);
  let mut tip = egui::Tooltip::for_enabled(r);
  tip.popup = tip.popup.frame(frame);
  tip.show(|ui| {
    ui.style_mut().visuals.override_text_color = Some(theme::TOOLTIP_TITLE);
    ui.spacing_mut().item_spacing.y = 3.0;
    for (icon, name, ok) in &lines {
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (ir, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
        icons::draw_color(ui.painter(), ir, *icon);
        ui.add(egui::Label::new(name.as_str()).extend());
        let (cr, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
        if *ok {
          check_mark(ui.painter(), cr);
        }
      });
    }
    let w = ui.min_rect().width().max(80.0);
    let (line, _) = ui.allocate_exact_size(vec2(w, 5.0), egui::Sense::hover());
    ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, theme::TOOLTIP_MUTED));
    ui.label(g.name.as_str());
  });
}

/// Fusion's green check mark.
fn check_mark(p: &egui::Painter, r: Rect) {
  let s = Stroke::new(2.0, theme::ICON_GREEN);
  let (a, b, c) =
    (r.left_center() + vec2(1.0, 0.0), r.center_bottom() + vec2(-1.0, -2.0), r.right_top() + vec2(-1.0, 2.0));
  p.line_segment([a, b], s);
  p.line_segment([b, c], s);
}

/// The small box on the track under a group: "+" under a collapsed group (expands it), "-" at
/// the middle of an expanded group's bracket (collapses it).
fn toggle_box(ui: &egui::Ui, id: egui::Id, at: egui::Pos2, open: bool) -> egui::Response {
  let rect = Rect::from_center_size(at, Vec2::splat(9.0));
  let r = ui.interact(rect, id, egui::Sense::click());
  let p = ui.painter();
  let ink = if r.hovered() { theme::ACCENT } else { Color32::from_gray(110) };
  p.rect(rect, 1.0, Color32::WHITE, Stroke::new(1.0, ink), egui::StrokeKind::Inside);
  let c = rect.center();
  p.line_segment([c - vec2(2.5, 0.0), c + vec2(2.5, 0.0)], Stroke::new(1.0, ink));
  if !open {
    p.line_segment([c - vec2(0.0, 2.5), c + vec2(0.0, 2.5)], Stroke::new(1.0, ink));
  }
  r
}

/// Fusion's group glyph: three small blue bars side by side.
fn group_glyph(p: &egui::Painter, r: Rect, ink: Color32) {
  let (w, gap) = (4.0, 2.0);
  let x0 = r.center().x - (3.0 * w + 2.0 * gap) / 2.0;
  for k in 0..3 {
    let x = x0 + k as f32 * (w + gap);
    let bar = Rect::from_min_max(egui::pos2(x, r.top() + 2.0), egui::pos2(x + w, r.bottom() - 2.0));
    p.rect_filled(bar, 1.0, ink);
  }
}

/// A collapsed group's icon: Fusion's three blue bars, a "+" box under it that expands the
/// group, its members' tooltip and the group's menu. A click (or right-click) picks the group.
/// Returns its rect.
pub(super) fn group_icon(app: &mut App, ui: &mut egui::Ui, g: &Shown, rolled_back: bool) -> Rect {
  let ids = members(app, g);
  let d = &app.s.design;
  let suppressed = ids.iter().all(|f| d.feature(*f).is_some_and(|f| f.suppressed));
  let failing = ids.iter().any(|f| app.s.model.status(*f).is_some_and(|s| s.error.is_some()));
  let selected = ids.iter().all(|f| app.s.selection.contains(&Sel::Feature { feature: *f }));
  let (rect, r) = ui.allocate_exact_size(vec2(26.0, 28.0), egui::Sense::click());
  let p = ui.painter();
  if failing {
    p.rect(rect, 2.0, Color32::from_rgb(253, 225, 222), Stroke::new(1.0, theme::ERROR), egui::StrokeKind::Inside);
  }
  if selected {
    p.rect(rect, 2.0, theme::SELECTED, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
  } else if r.hovered() {
    p.rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let ir = Rect::from_center_size(rect.center(), Vec2::splat(16.0));
  if suppressed {
    p.rect_filled(ir.expand(2.0), 1.0, super::SUPPRESSED_FILL);
  }
  let ink = if rolled_back || suppressed { Color32::from_gray(185) } else { theme::ICON_BLUE };
  group_glyph(p, ir, ink);
  group_tip(app, &r, g);
  if r.clicked() || r.secondary_clicked() {
    pick_group(app, g);
  }
  let plus = toggle_box(ui, egui::Id::new(("timeline-group-box", g.id)), rect.center_bottom() - vec2(0.0, 3.0), false);
  if plus.clicked() {
    set_open(ui.ctx(), g.id, true);
  }
  r.context_menu(|ui| group_menu(app, ui, g, false, suppressed));
  rect
}

/// The bracket under an expanded group's items, from its first to its last, with the "-" box at
/// its middle (click: collapse; right-click: the group's menu).
pub(super) fn bracket(app: &mut App, ui: &mut egui::Ui, g: &Shown, from: Rect, to: Rect) {
  let y = from.bottom() - 3.0;
  let (x0, x1) = (from.left() + 2.0, to.right() - 2.0);
  let s = Stroke::new(1.0, theme::ACCENT);
  let p = ui.painter();
  p.line_segment([egui::pos2(x0, y - 4.0), egui::pos2(x0, y)], s);
  p.line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], s);
  p.line_segment([egui::pos2(x1, y), egui::pos2(x1, y - 4.0)], s);
  let minus = toggle_box(ui, egui::Id::new(("timeline-group-box", g.id)), egui::pos2((x0 + x1) / 2.0, y), true);
  if minus.clicked() {
    set_open(ui.ctx(), g.id, false);
  }
  let suppressed = members(app, g).iter().all(|f| app.s.design.feature(*f).is_some_and(|f| f.suppressed));
  minus.context_menu(|ui| group_menu(app, ui, g, true, suppressed));
}

/// An entry of a group's right-click menu (see [`group_entries`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupEntry {
  /// The head shared with a multi-selection's menu: Create Selection Set, Extrude, Edit Sketch
  /// or Edit Feature, Configure.
  Head(TimelineEntry),
  /// Expand Group, or Collapse Group on an expanded one.
  Toggle,
  /// Fusion's red-X "Edit Group" with the Del key: deletes the group with its features.
  Delete,
  Rename,
  Suppress,
  /// Not in Fusion's collapsed group menu; kept on an expanded group's ("-" box) menu.
  Ungroup,
}

/// A group's right-click menu in Fusion's order (`None`: a separator): the multi-selection head
/// (Create Selection Set, Extrude (E), Edit Sketch, Configure for a group of sketches) | Expand
/// Group | Edit Group (Del), Rename | Suppress Features. An expanded group's menu (its "-" box)
/// reads Collapse Group and ends with Ungroup.
pub(crate) fn group_entries(app: &App, g: &Shown, open: bool) -> Vec<Option<GroupEntry>> {
  use GroupEntry as G;
  let mut v: Vec<Option<GroupEntry>> =
    super::several_head(app, &members(app, g)).into_iter().map(|e| e.map(G::Head)).collect();
  v.extend([None, Some(G::Toggle), None, Some(G::Delete), Some(G::Rename), None, Some(G::Suppress)]);
  if open {
    v.extend([None, Some(G::Ungroup)]);
  }
  v
}

impl GroupEntry {
  pub(crate) fn label(self, open: bool, suppressed: bool) -> &'static str {
    match self {
      GroupEntry::Head(e) => e.label(false),
      GroupEntry::Toggle if open => "Collapse Group",
      GroupEntry::Toggle => "Expand Group",
      GroupEntry::Delete => "Edit Group",
      GroupEntry::Rename => "Rename",
      GroupEntry::Suppress if suppressed => "Unsuppress Features",
      GroupEntry::Suppress => "Suppress Features",
      GroupEntry::Ungroup => "Ungroup",
    }
  }
}

/// A group's right-click menu, as in Fusion (see [`group_entries`]).
fn group_menu(app: &mut App, ui: &mut egui::Ui, g: &Shown, open: bool, suppressed: bool) {
  use GroupEntry as G;
  let ids = members(app, g);
  for entry in group_entries(app, g, open) {
    let Some(entry) = entry else {
      ui.separator();
      continue;
    };
    let label = entry.label(open, suppressed);
    match entry {
      G::Head(TimelineEntry::Extrude) => {
        if menu_item(ui, Icon::Extrude, label, Some("E")).clicked() {
          ui.close();
          super::sketch_rows::extrude_sketches(app, &ids);
        }
      }
      G::Head(TimelineEntry::EditSketch) => {
        if menu_item(ui, Icon::Sketch, label, None).clicked() {
          ui.close();
          app.edit_sketch(ids[0]);
        }
      }
      G::Head(TimelineEntry::EditFeature) => {
        let icon = app.s.design.feature(ids[0]).map_or(Icon::Folder, |f| super::feature_icon(&f.kind));
        if menu_item(ui, icon, label, None).clicked() {
          ui.close();
          super::edit_feature(app, ids[0]);
        }
      }
      // the members' suppression become columns of the configuration table
      G::Head(TimelineEntry::Configure) => {
        let first = app.s.design.configurations.list.first().map(|c| c.name.clone());
        let r = ui.add_enabled_ui(first.is_some(), |ui| menu_item(ui, Icon::ConfigTable, label, None)).inner;
        if r.clicked()
          && let Some(first) = first
        {
          ui.close();
          for f in &ids {
            let s = app.s.design.feature(*f).is_some_and(|f| f.suppressed);
            app.run("config.set", json!({"configuration": first, "feature": f.0, "suppressed": s}));
          }
          crate::config_ui::open_table();
        }
      }
      G::Head(_) => {
        ui.add_enabled_ui(false, |ui| menu_item(ui, Icon::Select, label, None));
      }
      G::Toggle => {
        if menu_item(ui, Icon::Folder, label, None).clicked() {
          ui.close();
          set_open(ui.ctx(), g.id, !open);
        }
      }
      G::Delete => {
        // menu_item's row with Fusion's red X in the icon slot
        let r = ui.add(egui::Button::new(format!("       {label}")).shortcut_text("Del").min_size(vec2(170.0, 0.0)));
        let x = Rect::from_center_size(r.rect.left_center() + vec2(13.0, 0.0), Vec2::splat(13.0));
        crate::error_dialog::error_glyph(ui.painter(), x);
        if r.clicked() {
          ui.close();
          app.run("timeline.group_delete", json!({"group": g.id}));
        }
      }
      G::Rename => {
        if menu_item(ui, Icon::Params, label, None).clicked() {
          ui.close();
          app.rename = Some((RenameTarget::Group(g.id), g.name.clone()));
        }
      }
      G::Suppress => {
        if menu_item(ui, Icon::EyeOff, label, None).clicked() {
          ui.close();
          app.run("timeline.group_suppress", json!({"group": g.id, "suppressed": !suppressed}));
        }
      }
      G::Ungroup => {
        if menu_item(ui, Icon::Folder, label, None).clicked() {
          ui.close();
          app.run("timeline.ungroup", json!({"group": g.id}));
        }
      }
    }
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

  /// Three sketches on the XY plane, each a rectangle.
  fn sketches() -> (App, Vec<FeatureId>) {
    let mut app = App::with_session(Session::default());
    for k in 0..3 {
      app.create_sketch(json!("xy"));
      app.run("sketch.rect", json!({"corner1": [k * 20, 0], "corner2": [k * 20 + 10, 5]}));
      app.finish_sketch();
    }
    let ids = app.s.design.features.iter().map(|f| f.id).collect();
    (app, ids)
  }

  fn labels(app: &App, id: FeatureId) -> Vec<String> {
    super::super::timeline_entries(app, id, false)
      .into_iter()
      .map(|e| {
        e.map_or("--".into(), |e| format!("{}{}", e.label(false), e.key().map_or(String::new(), |k| format!(" ({k})"))))
      })
      .collect()
  }

  #[test]
  fn a_sketch_items_menu_is_fusions() {
    let (app, f) = sketches();
    assert_eq!(
      labels(&app, f[0]),
      [
        "Create Selection Set",
        "--",
        "Extrude (E)",
        "--",
        "Edit Sketch",
        "Redefine Sketch Plane",
        "Select Sketch Plane",
        "Configure",
        "--",
        "Delete (Del)",
        "Rename",
        "--",
        "Roll Timeline Marker Here",
        "Convert to DM Feature",
        "Suppress Features",
        "--",
        "Find in Browser",
        "Find in Window",
      ]
    );
  }

  #[test]
  fn a_single_items_menu_has_no_create_group_and_finds_its_body() {
    let (mut app, _) = three();
    let body = app.s.model.bodies[0].id;
    let edge = app.s.model.bodies[0].edges()[0].id;
    app.run("fillet", json!({"edges": [{"body": body.0, "edge": edge}], "radius": 1}));
    let f = app.s.design.features[0].id;
    let l = labels(&app, f);
    assert!(!l.contains(&"Create Group".to_string()), "{l:?}");
    assert_eq!(l[..3], ["Create Selection Set", "--", "Edit Feature"]);
    // the box's body, whose last change is the fillet's, is still found in the browser
    assert_eq!(super::super::browser_node(&app, f), Some(Sel::Body { body }));
  }

  #[test]
  fn a_multi_selections_menu_and_status_are_fusions() {
    let (mut app, f) = sketches();
    click_feature(&mut app, f[0], false, false);
    click_feature(&mut app, f[2], true, false);
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("3 Features"));
    assert_eq!(
      labels(&app, f[1]),
      [
        "Create Selection Set",
        "Extrude (E)",
        "Edit Sketch",
        "Configure",
        "--",
        "Create Group",
        "--",
        "Delete (Del)",
        "Rename",
        "--",
        "Roll Timeline Marker Here",
        "Convert to DM Feature",
        "Suppress Features",
      ]
    );
    // a right-click outside the pick gets the single item's menu
    click_feature(&mut app, f[1], false, false);
    assert!(labels(&app, f[1]).contains(&"Find in Browser".to_string()));
    // Extrude from three sketches picks all their profiles
    click_feature(&mut app, f[2], true, false);
    super::super::sketch_rows::extrude_sketches(&mut app, &f[1..]);
    assert_eq!(app.s.selection.iter().filter(|s| matches!(s, Sel::Profile { .. })).count(), 2);
  }

  #[test]
  fn a_groups_menu_tooltip_and_status_are_fusions() {
    let (mut app, f) = sketches();
    app.run("timeline.group", json!({ "features": f.iter().map(|f| f.0).collect::<Vec<_>>() }));
    let g = shown(&app).remove(0);
    let l: Vec<&str> =
      group_entries(&app, &g, false).into_iter().map(|e| e.map_or("--", |e| e.label(false, false))).collect();
    assert_eq!(
      l,
      [
        "Create Selection Set",
        "Extrude",
        "Edit Sketch",
        "Configure",
        "--",
        "Expand Group",
        "--",
        "Edit Group",
        "Rename",
        "--",
        "Suppress Features",
      ]
    );
    assert_eq!(group_entries(&app, &g, true).last(), Some(&Some(GroupEntry::Ungroup)));
    pick_group(&mut app, &g);
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("4 Features"), "the group counts with its members");
    let tip = tip_lines(&app, &g);
    assert_eq!(
      tip.iter().map(|t| (t.1.as_str(), t.2)).collect::<Vec<_>>(),
      [("Sketch1", true), ("Sketch2", true), ("Sketch3", true)]
    );
    app.run("feature.suppress", json!({"feature": f[1].0}));
    assert!(!tip_lines(&app, &g)[1].2, "a suppressed member has no check");
  }

  #[test]
  fn a_quick_modifier_click_keeps_its_modifiers() {
    // the event carries Shift while the frame's state (Shift already up) has none
    let ctx = egui::Context::default();
    let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(20.0, 20.0));
    let shift = egui::Modifiers { shift: true, ..Default::default() };
    let input = egui::RawInput {
      events: vec![egui::Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: shift,
      }],
      ..Default::default()
    };
    let mut m = egui::Modifiers::NONE;
    ctx.run_ui(input, |ui| m = crate::panels::click_modifiers(ui, rect)).drop_without_applying_deltas();
    assert!(m.shift);
    ctx
      .run_ui(egui::RawInput::default(), |ui| m = crate::panels::click_modifiers(ui, rect))
      .drop_without_applying_deltas();
    assert!(!m.shift);
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
