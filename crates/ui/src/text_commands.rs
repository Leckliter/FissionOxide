//! File ▸ View ▸ Show Text Commands (Ctrl+Alt+C): Fusion's TEXT COMMANDS panel along the bottom
//! of the window, under the timeline. A log of what was run and what came back, an input line,
//! and the Txt / Py / TS switch at its right.
//!
//! Txt runs FissionOxide's own commands, the ones `fissionoxide-cli` and scripts use: a command
//! name and its parameters as JSON (`box {"width": 10, "depth": 10, "height": 10}`; no JSON
//! means `{}`, and `help` lists them). FissionOxide has no Python or TypeScript interpreter, so
//! Py and TS are listed as Fusion lists them but greyed.

use egui::RichText;
use serde_json::Value;

use crate::{App, theme};

/// The interpreter the input line feeds, as Fusion's radio buttons name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
  #[default]
  Txt,
  Py,
  Ts,
}

impl Mode {
  pub const ALL: [Mode; 3] = [Mode::Txt, Mode::Py, Mode::Ts];

  pub fn label(self) -> &'static str {
    match self {
      Mode::Txt => "Txt",
      Mode::Py => "Py",
      Mode::Ts => "TS",
    }
  }

  /// Whether FissionOxide can run this mode's input.
  pub fn available(self) -> bool {
    self == Mode::Txt
  }
}

/// One line of the log: the text and whether it is an error.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
  pub text: String,
  pub error: bool,
}

pub struct TextCommands {
  /// The panel is shown (File ▸ View ▸ Show / Hide Text Commands): Fusion shows it at start.
  pub open: bool,
  /// Expanded from its header bar to the log and the input line.
  pub expanded: bool,
  pub mode: Mode,
  pub input: String,
  pub log: Vec<Line>,
  /// What was run, oldest first (Up and Down in the input line step through it).
  history: Vec<String>,
  /// Where Up has stepped back to in `history`.
  back: Option<usize>,
}

impl Default for TextCommands {
  fn default() -> TextCommands {
    TextCommands {
      open: true,
      expanded: false,
      mode: Mode::default(),
      input: String::new(),
      log: Vec::new(),
      history: Vec::new(),
      back: None,
    }
  }
}

/// The expanded panel's height, as Fusion docks it.
const HEIGHT: f32 = 150.0;
/// The header bar's height: all that shows until it is expanded.
pub(crate) const BAR_H: f32 = 24.0;

/// Splits a Txt line into the command and its parameters.
pub fn parse(line: &str) -> Result<(String, Value), String> {
  let line = line.trim();
  let (name, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
  let params = if rest.trim().is_empty() {
    Value::Object(Default::default())
  } else {
    serde_json::from_str(rest.trim()).map_err(|e| format!("The parameters aren't JSON: {e}"))?
  };
  Ok((name.to_string(), params))
}

/// Runs the input line in Txt mode, logging it and what came back.
pub fn submit(app: &mut App) {
  let line = std::mem::take(&mut app.text_commands.input);
  let line = line.trim().to_string();
  if line.is_empty() {
    return;
  }
  let t = &mut app.text_commands;
  t.back = None;
  t.history.push(line.clone());
  t.log.push(Line { text: format!("> {line}"), error: false });
  let out = match parse(&line) {
    Err(e) => Line { text: e, error: true },
    Ok((name, params)) => match app.run(&name, params) {
      Some(v) => match v.get("error").and_then(Value::as_str) {
        Some(e) => Line { text: e.to_string(), error: true },
        None => Line { text: v.to_string(), error: false },
      },
      None => {
        let e = app.status.as_ref().filter(|s| s.1).map(|s| s.0.clone());
        Line { text: e.unwrap_or_else(|| format!("{name}: failed")), error: true }
      }
    },
  };
  app.text_commands.log.push(out);
}

/// Up (`back`) or Down in the input line: the previous or next command run.
fn step_history(t: &mut TextCommands, back: bool) {
  if t.history.is_empty() {
    return;
  }
  let last = t.history.len() - 1;
  t.back = match (t.back, back) {
    (None, true) => Some(last),
    (Some(i), true) => Some(i.saturating_sub(1)),
    (Some(i), false) if i < last => Some(i + 1),
    (_, false) => None,
  };
  t.input = t.back.map(|i| t.history[i].clone()).unwrap_or_default();
}

/// The panel, docked along the bottom of the window (call before the timeline so it sits under
/// it): Fusion's TEXT COMMANDS header bar, which a click expands to the log and the input line.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
  if !app.text_commands.open {
    return;
  }
  let frame = egui::Frame::NONE.fill(theme::PANEL).stroke(egui::Stroke::new(1.0, theme::BORDER));
  // separate panels for the bar and the expanded panel: each keeps its own height
  let panel = if app.text_commands.expanded {
    egui::Panel::bottom("text-commands").resizable(true).default_size(HEIGHT).min_size(80.0)
  } else {
    egui::Panel::bottom("text-commands-bar").resizable(false).exact_size(BAR_H)
  };
  panel.frame(frame).show(ui, |ui| {
    ui.spacing_mut().item_spacing.y = 0.0;
    let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), BAR_H), egui::Sense::hover());
    header(ui, bar, &mut app.text_commands.expanded);
    if !app.text_commands.expanded {
      return;
    }
    ui.spacing_mut().item_spacing.y = 4.0;
    body(app, ui);
  });
}

/// The header bar: an arrow and TEXT COMMANDS; a click expands or collapses the panel.
fn header(ui: &mut egui::Ui, r: egui::Rect, expanded: &mut bool) {
  let resp = ui.interact(r, egui::Id::new("text-commands-header"), egui::Sense::click());
  let fill = if resp.hovered() { theme::HOVER } else { theme::FLOAT_HEADER };
  ui.painter().rect_filled(r, 0.0, fill);
  let c = r.left_center() + egui::vec2(12.0, 0.0);
  // pointing up while collapsed (it opens upwards), down while expanded
  let pts = if *expanded {
    vec![c + egui::vec2(-4.0, -2.0), c + egui::vec2(4.0, -2.0), c + egui::vec2(0.0, 3.0)]
  } else {
    vec![c + egui::vec2(-4.0, 2.0), c + egui::vec2(4.0, 2.0), c + egui::vec2(0.0, -3.0)]
  };
  ui.painter().add(egui::Shape::convex_polygon(pts, theme::GROUP_LABEL, egui::Stroke::NONE));
  ui.painter().text(
    r.left_center() + egui::vec2(24.0, 0.0),
    egui::Align2::LEFT_CENTER,
    "TEXT COMMANDS",
    egui::FontId::proportional(12.0),
    theme::GROUP_LABEL,
  );
  let tip = if *expanded { "Collapse Text Commands" } else { "Expand Text Commands" };
  if resp.on_hover_text(tip).clicked() {
    *expanded = !*expanded;
  }
}

/// The expanded panel under the header: the log, then the input line and Txt / Py / TS.
fn body(app: &mut App, ui: &mut egui::Ui) {
  // fixed rects inside the panel: whatever is drawn never asks the panel to grow
  const ROW_H: f32 = 28.0;
  let rest = ui.available_rect_before_wrap();
  // claimed whole, so the panel keeps its height
  ui.allocate_rect(rest, egui::Sense::hover());
  let row = egui::Rect::from_min_max(egui::pos2(rest.left(), rest.bottom() - ROW_H), rest.right_bottom());
  let log = egui::Rect::from_min_max(rest.min, egui::pos2(rest.right(), row.top()));
  ui.painter().hline(rest.x_range(), row.top(), egui::Stroke::new(1.0, theme::BORDER));
  let mut log_ui = ui.new_child(egui::UiBuilder::new().max_rect(log.shrink2(egui::vec2(8.0, 3.0))));
  egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(&mut log_ui, |ui| {
    for l in &app.text_commands.log {
      let col = if l.error { theme::ERROR } else { theme::TEXT };
      ui.add(egui::Label::new(RichText::new(&l.text).monospace().color(col)).wrap());
    }
  });
  let mut row_ui = ui.new_child(
    egui::UiBuilder::new()
      .max_rect(row.shrink2(egui::vec2(8.0, 3.0)))
      .layout(egui::Layout::left_to_right(egui::Align::Center)),
  );
  let ui = &mut row_ui;
  let modes_w = 150.0;
  let mode = app.text_commands.mode;
  let hint = if mode.available() { "Enter a command (help lists them)" } else { "" };
  let r = ui.add_enabled(
    mode.available(),
    egui::TextEdit::singleline(&mut app.text_commands.input)
      .id(egui::Id::new("text-commands-input"))
      .font(egui::TextStyle::Monospace)
      .hint_text(hint)
      .desired_width((ui.available_width() - modes_w).max(80.0)),
  );
  if r.has_focus() {
    let (up, down) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown)));
    if up || down {
      step_history(&mut app.text_commands, up);
    }
  }
  if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
    submit(app);
    r.request_focus();
  }
  for m in Mode::ALL {
    let b = ui.add_enabled(m.available(), egui::RadioButton::new(app.text_commands.mode == m, m.label()));
    let b = b.on_disabled_hover_text(format!(
      "FissionOxide has no {} interpreter; Txt runs its commands",
      if m == Mode::Py { "Python" } else { "TypeScript" }
    ));
    if b.clicked() {
      app.text_commands.mode = m;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;
  use serde_json::json;

  #[test]
  fn txt_lines_are_a_command_and_its_json() {
    assert_eq!(parse("box {\"width\": 2}").unwrap(), ("box".to_string(), json!({"width": 2})));
    assert_eq!(parse("  info ").unwrap(), ("info".to_string(), json!({})));
    assert!(parse("box {width").is_err());
  }

  #[test]
  fn txt_runs_commands_and_logs_them() {
    let mut app = App::with_session(Session::default());
    app.text_commands.input = r#"box {"width": 10, "depth": 10, "height": 10}"#.into();
    submit(&mut app);
    assert_eq!(app.s.model.bodies.len(), 1, "the box was made");
    assert_eq!(app.text_commands.log[0].text, r#"> box {"width": 10, "depth": 10, "height": 10}"#);
    assert!(!app.text_commands.log[1].error);
    assert!(app.text_commands.input.is_empty());
    // a refused command reads its error in red
    app.text_commands.input = "no.such.command".into();
    submit(&mut app);
    assert!(app.text_commands.log.last().unwrap().error, "{:?}", app.text_commands.log);
    // Up brings back what was run, newest first; Down goes forward again
    step_history(&mut app.text_commands, true);
    assert_eq!(app.text_commands.input, "no.such.command");
    step_history(&mut app.text_commands, true);
    assert!(app.text_commands.input.starts_with("box "));
    step_history(&mut app.text_commands, false);
    assert_eq!(app.text_commands.input, "no.such.command");
    step_history(&mut app.text_commands, false);
    assert!(app.text_commands.input.is_empty());
  }

  #[test]
  fn only_txt_can_run() {
    assert_eq!(Mode::ALL.map(|m| m.label()), ["Txt", "Py", "TS"]);
    assert!(Mode::Txt.available() && !Mode::Py.available() && !Mode::Ts.available());
  }
}
