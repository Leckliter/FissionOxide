//! The right-click marking menu, as in Fusion: eight commands arranged around the cursor (pick
//! one by clicking it, or by clicking anywhere in its direction) and an overflow list of
//! commands for the current selection underneath.

use clf_engine::Sel;
use clf_engine::kernel::SurfaceKind;
use clf_engine::model::FeatureId;
use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::dialogs::Kind;
use crate::icons::{self, Icon};
use crate::panels::{self, Cmd};
use crate::{App, Tool, theme};

/// Where the item ring sits from the centre (the vertical spacing is squeezed, as in Fusion).
const RADIUS: f32 = 112.0;
/// Clicks within this distance of the centre dismiss the menu.
const DEAD_ZONE: f32 = 22.0;
/// Directional picks reach this far; clicks further out dismiss the menu.
const REACH: f32 = 260.0;

pub struct MarkingMenu {
  center: Pos2,
}

#[derive(Clone, Copy)]
enum Action {
  Cmd(Cmd),
  Repeat(Cmd),
  EditFeature(FeatureId),
}

struct Item {
  action: Action,
  icon: Icon,
  label: String,
}

impl Item {
  fn cmd(c: Cmd) -> Item {
    let (icon, label, _) = c.info();
    Item { action: Action::Cmd(c), icon, label: label.into() }
  }

  fn with_label(c: Cmd, label: &str) -> Item {
    let (icon, _, _) = c.info();
    Item { action: Action::Cmd(c), icon, label: label.into() }
  }
}

/// Opens the menu at `pos`. A right-click on unselected geometry selects it first, so the
/// overflow list offers commands for what was clicked.
pub fn open(app: &mut App, pos: Pos2) {
  if let Some(h) = app.hover {
    let sel = h.to_sel();
    if !app.s.selection.contains(&sel) {
      app.s.selection = vec![sel];
    }
  }
  app.nav_mode = None;
  app.marking = Some(MarkingMenu { center: pos });
}

/// The eight ring items, clockwise from north. `None` leaves a direction empty.
fn ring(app: &App) -> [Option<Item>; 8] {
  let repeat = app.last_cmd.map(|c| {
    let (icon, name, _) = c.info();
    Item { action: Action::Repeat(c), icon, label: format!("Repeat {name}") }
  });
  if app.s.edit_sketch.is_some() {
    [
      repeat,
      Some(Item::cmd(Cmd::Delete)),
      Some(Item::cmd(Cmd::Redo)),
      Some(Item::with_label(Cmd::SketchTool(Tool::Circle), "Circle")),
      Some(Item::cmd(Cmd::SketchTool(Tool::Dimension))),
      Some(Item::with_label(Cmd::SketchTool(Tool::Rect), "Rectangle")),
      Some(Item::cmd(Cmd::Undo)),
      Some(Item::cmd(Cmd::SketchTool(Tool::Line))),
    ]
  } else {
    [
      repeat,
      Some(Item::cmd(Cmd::Delete)),
      Some(Item::cmd(Cmd::Redo)),
      Some(Item::cmd(Cmd::Dialog(Kind::Fillet))),
      Some(Item::cmd(Cmd::NewSketch)),
      Some(Item::cmd(Cmd::Dialog(Kind::Extrude))),
      Some(Item::cmd(Cmd::Undo)),
      Some(Item::cmd(Cmd::Dialog(Kind::Move))),
    ]
  }
}

/// The overflow list: commands for the selection, then general ones (`None` is a separator).
fn overflow(app: &App) -> Vec<Option<Item>> {
  let mut v: Vec<Option<Item>> = Vec::new();
  if app.s.edit_sketch.is_some() {
    v.push(Some(Item::cmd(Cmd::FinishSketch)));
    v.push(None);
    if app.s.selection.iter().any(|s| matches!(s, Sel::SketchCurve { .. })) {
      v.push(Some(Item::with_label(Cmd::Construction, "Normal/Construction")));
    }
    v.push(Some(Item::cmd(Cmd::LookAt)));
    v.push(Some(Item::cmd(Cmd::Fit)));
    v.push(None);
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Inspect))));
    return v;
  }
  let sel = &app.s.selection;
  let planar_face = sel.iter().any(|s| match s {
    Sel::Face { body, face } => app
      .s
      .model
      .body(*body)
      .and_then(|b| b.faces().iter().find(|f| f.id == *face).map(|f| f.kind == SurfaceKind::Plane))
      .unwrap_or(false),
    _ => false,
  });
  let any_edge = sel.iter().any(|s| matches!(s, Sel::Edge { .. }));
  let any_body = sel.iter().any(|s| matches!(s, Sel::Body { .. }));
  let feature = sel.iter().find_map(|s| if let Sel::Feature { feature } = s { Some(*feature) } else { None });
  let before = v.len();
  if let Some(f) = feature {
    let name = app.s.design.feature(f).map(|f| f.name.clone()).unwrap_or_default();
    v.push(Some(Item { action: Action::EditFeature(f), icon: Icon::Gear, label: format!("Edit {name}") }));
  }
  if planar_face {
    v.push(Some(Item::cmd(Cmd::NewSketch)));
    v.push(Some(Item::cmd(Cmd::LookAt)));
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Shell))));
  }
  if any_edge {
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Fillet))));
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Chamfer))));
  }
  if any_body {
    v.push(Some(Item::cmd(Cmd::ToggleVisibility)));
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Move))));
    v.push(Some(Item::with_label(Cmd::Dialog(Kind::Inspect), "Properties")));
  }
  if v.len() > before {
    v.push(None);
  }
  v.push(Some(Item::cmd(Cmd::Dialog(Kind::Inspect))));
  v.push(Some(Item::cmd(Cmd::Params)));
  v.push(None);
  v.push(Some(Item::cmd(Cmd::Fit)));
  v.push(Some(Item::cmd(Cmd::Home)));
  v
}

/// Unit direction of ring slot `i` (0 = north, clockwise).
fn dir(i: usize) -> Vec2 {
  let a = i as f32 * std::f32::consts::FRAC_PI_4;
  vec2(a.sin(), -a.cos())
}

/// The box of ring item `i`: left-aligned on the right side, right-aligned on the left side,
/// centred at north and south.
fn item_rect(center: Pos2, i: usize, size: Vec2) -> Rect {
  let d = dir(i);
  let anchor = center + vec2(d.x * RADIUS, d.y * RADIUS * 0.72);
  let min_x = if d.x > 0.1 {
    anchor.x - 14.0
  } else if d.x < -0.1 {
    anchor.x - size.x + 14.0
  } else {
    anchor.x - size.x / 2.0
  };
  Rect::from_min_size(pos2(min_x, anchor.y - size.y / 2.0), size)
}

/// The ring slot a pointer at `p` points to, if it's outside the dead zone and within reach.
fn slot_at(center: Pos2, p: Pos2) -> Option<usize> {
  let v = p - center;
  let len = v.length();
  if !(DEAD_ZONE..=REACH).contains(&len) {
    return None;
  }
  let a = v.x.atan2(-v.y).rem_euclid(std::f32::consts::TAU);
  Some(((a / std::f32::consts::FRAC_PI_4).round() as usize) % 8)
}

pub fn show(app: &mut App, ctx: &egui::Context) {
  let Some(center) = app.marking.as_ref().map(|m| m.center) else { return };
  let ring = ring(app);
  let overflow = overflow(app);
  let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("marking-menu")));
  let font = egui::FontId::proportional(12.5);
  let (pointer, clicked, secondary) =
    ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked(), i.pointer.secondary_clicked()));

  // layout
  let ring_rects: Vec<Option<Rect>> = ring
    .iter()
    .enumerate()
    .map(|(i, it)| {
      it.as_ref().map(|it| {
        let tw = painter.layout_no_wrap(it.label.clone(), font.clone(), theme::TEXT).size().x;
        item_rect(center, i, vec2(tw + 40.0, 26.0))
      })
    })
    .collect();
  let list_w = overflow
    .iter()
    .flatten()
    .map(|it| painter.layout_no_wrap(it.label.clone(), font.clone(), theme::TEXT).size().x + 48.0)
    .fold(180.0f32, f32::max);
  let list_h: f32 = overflow.iter().map(|it| if it.is_some() { 24.0 } else { 7.0 }).sum::<f32>() + 6.0;
  let mut list_min = center + vec2(-list_w / 2.0, RADIUS * 0.72 + 24.0);
  let canvas = app.canvas;
  if list_min.y + list_h > canvas.bottom() - 4.0 {
    // no room below: put the list to the right of the ring
    list_min = pos2(center.x + RADIUS + 70.0, center.y - list_h / 2.0);
  }
  list_min.x = list_min.x.clamp(canvas.left() + 4.0, (canvas.right() - list_w - 4.0).max(canvas.left() + 4.0));
  list_min.y = list_min.y.clamp(canvas.top() + 4.0, (canvas.bottom() - list_h - 4.0).max(canvas.top() + 4.0));
  let list_rect = Rect::from_min_size(list_min, vec2(list_w, list_h));

  // what the pointer points at: an item box first, then the list, then a direction
  let mut hot_slot: Option<usize> = None;
  let mut hot_row: Option<usize> = None;
  if let Some(p) = pointer {
    hot_slot = ring_rects.iter().position(|r| r.is_some_and(|r| r.contains(p)));
    if hot_slot.is_none() && list_rect.contains(p) {
      let mut y = list_rect.top() + 3.0;
      for (k, it) in overflow.iter().enumerate() {
        let h = if it.is_some() { 24.0 } else { 7.0 };
        if it.is_some() && p.y >= y && p.y < y + h {
          hot_row = Some(k);
        }
        y += h;
      }
    } else if hot_slot.is_none() {
      hot_slot = slot_at(center, p).filter(|s| ring[*s].is_some());
    }
  }

  // centre: a small hub with the pointed-at direction lit
  painter.circle(center, DEAD_ZONE, Color32::from_white_alpha(170), Stroke::new(1.0, Color32::from_gray(150)));
  painter.circle_filled(center, 3.0, Color32::from_gray(110));
  for (i, it) in ring.iter().enumerate() {
    if it.is_none() {
      continue;
    }
    let d = dir(i);
    let col = if hot_slot == Some(i) { theme::ACCENT } else { Color32::from_gray(185) };
    let w = if hot_slot == Some(i) { 4.0 } else { 2.0 };
    let a = d.y.atan2(d.x);
    let arc: Vec<Pos2> = (0..=8)
      .map(|k| {
        let t = a - 0.3 + 0.6 * k as f32 / 8.0;
        center + vec2(t.cos(), t.sin()) * (DEAD_ZONE + 3.0)
      })
      .collect();
    painter.add(egui::Shape::line(arc, Stroke::new(w, col)));
  }
  if let Some(i) = hot_slot
    && let Some(r) = ring_rects[i]
  {
    let from = center + dir(i) * (DEAD_ZONE + 5.0);
    let to = pos2(center.x.clamp(r.left(), r.right()), center.y.clamp(r.top(), r.bottom()));
    painter.line_segment([from, to], Stroke::new(1.0, theme::ACCENT));
  }

  // ring items
  for (i, it) in ring.iter().enumerate() {
    let (Some(it), Some(r)) = (it, ring_rects[i]) else { continue };
    let hot = hot_slot == Some(i);
    let (fill, stroke, text) = if hot {
      (theme::ACCENT, theme::ACCENT_DARK, Color32::WHITE)
    } else {
      (Color32::WHITE, Color32::from_gray(165), theme::TEXT)
    };
    painter.rect(
      r.translate(vec2(0.0, 1.0)),
      2.0,
      Color32::from_black_alpha(30),
      Stroke::NONE,
      egui::StrokeKind::Inside,
    );
    painter.rect(r, 2.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let ir = Rect::from_center_size(r.left_center() + vec2(15.0, 0.0), Vec2::splat(17.0));
    if hot {
      painter.rect_filled(ir.expand(2.0), 2.0, Color32::WHITE);
    }
    icons::draw_color(&painter, ir, it.icon);
    painter.text(r.left_center() + vec2(30.0, 0.0), egui::Align2::LEFT_CENTER, &it.label, font.clone(), text);
  }

  // overflow list
  painter.rect(
    list_rect.translate(vec2(0.0, 2.0)).expand(1.0),
    2.0,
    Color32::from_black_alpha(25),
    Stroke::NONE,
    egui::StrokeKind::Outside,
  );
  painter.rect(list_rect, 2.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(165)), egui::StrokeKind::Inside);
  let mut y = list_rect.top() + 3.0;
  for (k, it) in overflow.iter().enumerate() {
    match it {
      Some(it) => {
        let row = Rect::from_min_size(pos2(list_rect.left() + 1.0, y), vec2(list_w - 2.0, 24.0));
        if hot_row == Some(k) {
          painter.rect_filled(row, 0.0, theme::HOVER);
        }
        icons::draw_color(
          &painter,
          Rect::from_center_size(row.left_center() + vec2(16.0, 0.0), Vec2::splat(16.0)),
          it.icon,
        );
        painter.text(
          row.left_center() + vec2(32.0, 0.0),
          egui::Align2::LEFT_CENTER,
          &it.label,
          font.clone(),
          theme::TEXT,
        );
        y += 24.0;
      }
      None => {
        painter.line_segment(
          [pos2(list_rect.left() + 8.0, y + 3.5), pos2(list_rect.right() - 8.0, y + 3.5)],
          Stroke::new(1.0, theme::SEPARATOR),
        );
        y += 7.0;
      }
    }
  }

  // input
  if secondary {
    // a second right-click moves the menu (handled by the viewport) or dismisses it here
    if !pointer.is_some_and(|p| canvas.contains(p)) {
      app.marking = None;
    }
    return;
  }
  if !clicked {
    return;
  }
  let picked = if !pointer.is_some_and(|p| canvas.contains(p)) {
    None
  } else if let Some(k) = hot_row {
    overflow[k].as_ref().map(|it| it.action)
  } else {
    hot_slot.and_then(|i| ring[i].as_ref().map(|it| it.action))
  };
  app.marking = None;
  match picked {
    Some(Action::Cmd(c)) | Some(Action::Repeat(c)) => c.run(app),
    Some(Action::EditFeature(f)) => panels::edit_feature(app, f),
    None => {}
  }
}
