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

#[derive(Default)]
pub struct TextCommands {
  /// The panel is shown (File ▸ View ▸ Show / Hide Text Commands).
  pub open: bool,
  pub mode: Mode,
  pub input: String,
  pub log: Vec<Line>,
  /// What was run, oldest first (Up and Down in the input line step through it).
  history: Vec<String>,
  /// Where Up has stepped back to in `history`.
  back: Option<usize>,
}

/// The panel's height, as Fusion docks it.
const HEIGHT: f32 = 150.0;

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
/// it).
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
  if !app.text_commands.open {
    return;
  }
  egui::Panel::bottom("text-commands")
    .resizable(true)
    .default_size(HEIGHT)
    .min_size(80.0)
    .frame(egui::Frame::NONE.fill(theme::PANEL).stroke(egui::Stroke::new(1.0, theme::BORDER)))
    .show(ui, |ui| {
      // the title bar: TEXT COMMANDS and the close button
      egui::Frame::NONE.fill(theme::FLOAT_HEADER).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
          ui.label(RichText::new("TEXT COMMANDS").size(11.5).strong().color(theme::GROUP_LABEL));
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = ui.add(egui::Button::new(RichText::new("×").size(14.0).color(theme::MUTED)).frame(false));
            if close.on_hover_text("Hide Text Commands").clicked() {
              app.text_commands.open = false;
            }
          });
        });
      });
      let row_h = 26.0;
      let log_h = (ui.available_height() - row_h - 6.0).max(20.0);
      egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(log_h).auto_shrink([false, false]).stick_to_bottom(true).show(
          ui,
          |ui| {
            for l in &app.text_commands.log {
              let col = if l.error { theme::ERROR } else { theme::TEXT };
              ui.add(egui::Label::new(RichText::new(&l.text).monospace().color(col)).wrap());
            }
          },
        );
      });
      ui.separator();
      ui.horizontal(|ui| {
        ui.add_space(8.0);
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
      });
      ui.add_space(2.0);
    });
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
