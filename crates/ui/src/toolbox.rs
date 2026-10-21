//! The S toolbox ("Design Shortcuts"), as in Fusion: S over the canvas opens a small box at the
//! pointer with the commands added to it (Add to Shortcuts), the recently used ones and a search
//! field. Typing lists every command of
//! the workspace whose name matches; arrows and Enter, or a click, run one.

use egui::{Color32, Key, Modifiers, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::icons;
use crate::menus::{self, Entry};
use crate::panels::{Cmd, Tab};
use crate::tips::Scope;
use crate::{App, theme};

/// How many recent commands the box remembers and shows.
pub(crate) const RECENT: usize = 8;
/// Most matches listed.
const MAX_ROWS: usize = 12;
const WIDTH: f32 = 300.0;

pub(crate) struct Toolbox {
  pos: Pos2,
  query: String,
  /// The highlighted match.
  sel: usize,
}

impl Toolbox {
  pub(crate) fn at(pos: Pos2) -> Toolbox {
    Toolbox { pos, query: String::new(), sel: 0 }
  }
}

/// The toolbar tabs whose commands the box offers: the active workspace's, the open sketch's
/// first, then the tab showing, then the rest of the Design workspace.
pub(crate) fn workspace_tabs(app: &App) -> Vec<Tab> {
  if app.drawing.is_some() {
    return vec![Tab::Drawing];
  }
  if app.generative {
    return vec![Tab::Generative];
  }
  if app.render.active {
    return vec![Tab::Render];
  }
  if app.manufacture {
    return vec![Tab::Milling, Tab::Turning];
  }
  let mut tabs = Vec::new();
  if app.s.edit_sketch.is_some() {
    tabs.push(Tab::Sketch);
  }
  if app.tab != Tab::Sketch {
    tabs.push(app.tab);
  }
  for t in [Tab::Solid, Tab::Surface, Tab::Mesh, Tab::SheetMetal, Tab::Plastic, Tab::Manage, Tab::Utilities] {
    if !tabs.contains(&t) {
      tabs.push(t);
    }
  }
  tabs
}

/// The commands whose name matches `query` (ignoring case), best first: names starting with
/// it, then names with a word starting with it, then names containing it. Toolbar order breaks
/// ties.
pub(crate) fn search(entries: &[Entry], query: &str) -> Vec<Entry> {
  let q = query.trim().to_lowercase();
  if q.is_empty() {
    return Vec::new();
  }
  let mut hits: Vec<(u8, usize, Entry)> = entries
    .iter()
    .enumerate()
    .filter_map(|(i, e)| {
      let name = e.label.to_lowercase();
      let rank = if name.starts_with(&q) {
        0
      } else if name.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q)) {
        1
      } else if name.contains(&q) {
        2
      } else {
        return None;
      };
      Some((rank, i, *e))
    })
    .collect();
  hits.sort_by_key(|(rank, i, _)| (*rank, *i));
  // a command found on several tabs is listed once
  let mut out: Vec<Entry> = Vec::new();
  for (_, _, e) in hits {
    if !out.iter().any(|o| o.label == e.label && o.cmd == e.cmd) {
      out.push(e);
    }
  }
  out
}

pub fn show(app: &mut App, ctx: &egui::Context) {
  let Some(mut tb) = app.toolbox.take() else { return };
  let tabs = workspace_tabs(app);
  let entries = menus::commands(&tabs);
  let recent = shortcut_row(app, &entries);
  let hits = search(&entries, &tb.query);
  let keys: Vec<String> =
    hits.iter().take(MAX_ROWS).map(|e| menus::key_of(&app.prefs, Scope::of(e.tab), e.label).to_string()).collect();

  // keys, taken before the search field sees them
  let (up, down, enter, esc) = ctx.input_mut(|i| {
    (
      i.consume_key(Modifiers::NONE, Key::ArrowUp),
      i.consume_key(Modifiers::NONE, Key::ArrowDown),
      i.consume_key(Modifiers::NONE, Key::Enter),
      i.consume_key(Modifiers::NONE, Key::Escape),
    )
  });
  let rows = hits.len().min(MAX_ROWS);
  if rows > 0 {
    if down {
      tb.sel = (tb.sel + 1) % rows;
    }
    if up {
      tb.sel = (tb.sel + rows - 1) % rows;
    }
  }
  tb.sel = tb.sel.min(rows.saturating_sub(1));
  let mut run: Option<Cmd> = if enter { hits.get(tb.sel).map(|e| e.cmd) } else { None };

  let title = if app.drawing.is_some() {
    "DRAWING SHORTCUTS"
  } else if app.manufacture {
    "MANUFACTURE SHORTCUTS"
  } else {
    "DESIGN SHORTCUTS"
  };
  let query_before = tb.query.clone();
  let area = egui::Area::new(egui::Id::new("s-toolbox"))
    .order(egui::Order::Foreground)
    .fixed_pos(tb.pos)
    .constrain(true)
    .show(ctx, |ui| {
      egui::Frame::popup(ui.style()).fill(Color32::WHITE).inner_margin(6.0).show(ui, |ui| {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
        ui.label(egui::RichText::new(title).size(11.0).color(theme::GROUP_LABEL));
        if !recent.is_empty() {
          ui.horizontal(|ui| {
            for e in &recent {
              let (r, resp) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::click());
              if resp.hovered() {
                ui.painter().rect_filled(r, 2.0, theme::HOVER);
              }
              icons::draw_tinted(ui.painter(), r.shrink(4.0), e.icon, e.tint);
              if resp.on_hover_text(e.label).clicked() {
                run = Some(e.cmd);
              }
            }
          });
        }
        let field =
          ui.add(egui::TextEdit::singleline(&mut tb.query).hint_text("Search commands").desired_width(f32::INFINITY));
        field.request_focus();
        if tb.query.trim().is_empty() {
          return;
        }
        if hits.is_empty() {
          ui.label(egui::RichText::new("No matching commands").color(Color32::from_gray(130)));
          return;
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        for (k, e) in hits.iter().take(MAX_ROWS).enumerate() {
          if row(ui, e, &keys[k], k == tb.sel).clicked() {
            run = Some(e.cmd);
          }
        }
        if hits.len() > MAX_ROWS {
          ui.label(
            egui::RichText::new(format!("{} more — keep typing", hits.len() - MAX_ROWS))
              .size(11.0)
              .color(Color32::from_gray(130)),
          );
        }
      });
    });
  if tb.query != query_before {
    tb.sel = 0;
  }
  let pressed_outside =
    ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
  if let Some(c) = run {
    app.launch(c);
  } else if !(esc || pressed_outside) {
    app.toolbox = Some(tb);
  }
}

/// The box's icon row: the commands the user added to it, then the recent ones.
fn shortcut_row(app: &App, entries: &[Entry]) -> Vec<Entry> {
  let added = app.prefs.shortcuts.iter().filter_map(|id| entries.iter().find(|e| e.id() == *id));
  let recent = app.recent.iter().filter_map(|c| entries.iter().find(|e| e.cmd == *c));
  let mut out: Vec<Entry> = Vec::new();
  for e in added.chain(recent) {
    if !out.iter().any(|o| o.cmd == e.cmd) {
      out.push(*e);
    }
  }
  out.truncate(RECENT.max(app.prefs.shortcuts.len()));
  out
}

/// One match: icon, name, and the shortcut key or the tab it comes from.
fn row(ui: &mut egui::Ui, e: &Entry, key: &str, hot: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, 24.0), egui::Sense::click());
  let p = ui.painter();
  if hot || resp.hovered() {
    p.rect_filled(rect, 0.0, theme::HOVER);
  }
  if hot {
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
  }
  let ir = Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), Vec2::splat(17.0));
  icons::draw_tinted(p, ir, e.icon, e.tint);
  p.text(
    rect.left_center() + vec2(30.0, 0.0),
    egui::Align2::LEFT_CENTER,
    e.label,
    egui::FontId::proportional(12.5),
    theme::TEXT,
  );
  let hint = if key.is_empty() { e.tab.label().to_string() } else { key.to_string() };
  let color = if key.is_empty() { Color32::from_gray(150) } else { theme::TEXT };
  p.text(
    pos2(rect.right() - 6.0, rect.center().y),
    egui::Align2::RIGHT_CENTER,
    hint,
    egui::FontId::proportional(11.0),
    color,
  );
  resp
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dialogs::Kind;
  use fox_engine::Session;
  use serde_json::json;

  fn labels(hits: &[Entry]) -> Vec<&'static str> {
    hits.iter().map(|e| e.label).collect()
  }

  #[test]
  fn typing_sh_lists_shell_and_sheet_metal_rules() {
    let app = App::with_session(Session::default());
    let hits = search(&menus::commands(&workspace_tabs(&app)), "sh");
    let l = labels(&hits);
    assert!(l.contains(&"Shell") && l.contains(&"Sheet Metal Rules"), "{l:?}");
    // names starting with the text come before ones that only contain it
    assert!(l[0].to_lowercase().starts_with("sh"), "{l:?}");
    assert!(search(&menus::commands(&workspace_tabs(&app)), "  ").is_empty());
    assert!(search(&menus::commands(&workspace_tabs(&app)), "zzzz").is_empty());
  }

  fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput { events, ..Default::default() };
    ctx.run_ui(input, |ui| show(app, ui.ctx())).drop_without_applying_deltas();
  }

  fn key(key: Key) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
  }

  #[test]
  fn typing_then_arrows_and_enter_run_a_command() {
    let ctx = egui::Context::default();
    let mut app = App::with_session(Session::default());
    app.toolbox = Some(Toolbox::at(pos2(200.0, 200.0)));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![egui::Event::Text("cha".into())]);
    assert_eq!(app.toolbox.as_ref().map(|t| t.query.as_str()), Some("cha"));
    // Chamfer, then Change Parameters
    frame(&mut app, &ctx, vec![key(Key::ArrowDown)]);
    frame(&mut app, &ctx, vec![key(Key::ArrowUp)]);
    frame(&mut app, &ctx, vec![key(Key::Enter)]);
    assert!(app.toolbox.is_none());
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), Some(Kind::Chamfer));
    assert_eq!(app.recent.first(), Some(&Cmd::Dialog(Kind::Chamfer)));
    // Escape closes it without running anything
    app.toolbox = Some(Toolbox::at(pos2(200.0, 200.0)));
    frame(&mut app, &ctx, vec![key(Key::Escape)]);
    assert!(app.toolbox.is_none());
  }

  #[test]
  fn the_box_offers_the_active_workspace() {
    let mut app = App::with_session(Session::default());
    let design = menus::commands(&workspace_tabs(&app));
    assert!(design.iter().any(|e| e.cmd == Cmd::Dialog(Kind::Fillet)));
    assert!(!design.iter().any(|e| matches!(e.cmd, Cmd::DrawingExport(_))));
    // in a sketch its own tools come first, and the modelling ones are still there
    app.run("sketch.create", json!({"plane": "xy"}));
    let sketch = menus::commands(&workspace_tabs(&app));
    assert_eq!(sketch[0].tab, Tab::Sketch);
    let line = search(&sketch, "line");
    assert_eq!(line[0].cmd, Cmd::SketchTool(crate::Tool::Line));
    assert!(sketch.iter().any(|e| e.cmd == Cmd::Dialog(Kind::Extrude)));
    app.drawing = Some(1);
    let drawing = menus::commands(&workspace_tabs(&app));
    assert!(drawing.iter().all(|e| e.tab == Tab::Drawing));
  }
}
