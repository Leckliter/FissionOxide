//! `settings.ini` in [`crate::config_dir`]. The Windows installer writes the `[Updates]` choice
//! the user made in its wizard; the app reads it and writes it back when changed in Help →
//! Check for Updates. Unknown sections and keys are kept as they are.

use std::path::{Path, PathBuf};

/// What the app does about new releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateMode {
  /// Download in the background and install when the app closes (or straight away on request).
  #[default]
  Auto,
  /// Tell the user; download and install only when asked.
  Notify,
  /// Never look; Check for Updates still works.
  Off,
}

impl UpdateMode {
  pub const ALL: [UpdateMode; 3] = [UpdateMode::Auto, UpdateMode::Notify, UpdateMode::Off];

  pub fn key(self) -> &'static str {
    match self {
      UpdateMode::Auto => "auto",
      UpdateMode::Notify => "notify",
      UpdateMode::Off => "off",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      UpdateMode::Auto => "Download and install updates automatically",
      UpdateMode::Notify => "Tell me when an update is available",
      UpdateMode::Off => "Don't check for updates",
    }
  }

  fn parse(s: &str) -> Option<UpdateMode> {
    UpdateMode::ALL.into_iter().find(|m| m.key().eq_ignore_ascii_case(s.trim()))
  }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
  pub update_mode: UpdateMode,
  /// When updates were last looked for (seconds since 1970).
  pub last_check: u64,
  /// A version the user chose to skip: not offered again, though later ones are.
  pub skip_version: Option<String>,
  /// The whole file as read, so writing it back keeps what this struct doesn't know.
  lines: Vec<String>,
}

const SECTION: &str = "Updates";

impl Settings {
  pub fn path() -> PathBuf {
    crate::config_dir().join("settings.ini")
  }

  /// Reads the settings, or the defaults when the file is missing or unreadable.
  pub fn load() -> Settings {
    Self::load_from(&Self::path())
  }

  pub fn load_from(path: &Path) -> Settings {
    std::fs::read_to_string(path).map(|s| Self::parse(&s)).unwrap_or_default()
  }

  pub fn parse(text: &str) -> Settings {
    // a UTF-8 byte order mark, as Windows editors write
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut s = Settings { lines: text.lines().map(str::to_string).collect(), ..Settings::default() };
    let mut section = String::new();
    for line in &s.lines {
      let line = line.trim();
      if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
        section = name.trim().to_string();
      } else if section.eq_ignore_ascii_case(SECTION)
        && let Some((k, v)) = line.split_once('=')
      {
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
          "mode" => s.update_mode = UpdateMode::parse(v).unwrap_or_default(),
          "lastcheck" => s.last_check = v.parse().unwrap_or(0),
          "skip" => s.skip_version = Some(v.to_string()).filter(|v| !v.is_empty()),
          _ => {}
        }
      }
    }
    s
  }

  /// The file text: the lines read, with the `[Updates]` keys set to the current values.
  pub fn to_text(&self) -> String {
    let values = [
      ("Mode", self.update_mode.key().to_string()),
      ("LastCheck", self.last_check.to_string()),
      ("Skip", self.skip_version.clone().unwrap_or_default()),
    ];
    let mut out: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut written = [false; 3];
    let mut found = false;
    let flush = |out: &mut Vec<String>, written: &mut [bool; 3]| {
      // keys the section didn't have go after its last non-blank line
      let at = out.iter().rposition(|l| !l.trim().is_empty()).map_or(out.len(), |i| i + 1);
      for (i, (k, v)) in values.iter().enumerate().rev() {
        if !written[i] {
          out.insert(at, format!("{k}={v}"));
          written[i] = true;
        }
      }
    };
    for line in &self.lines {
      let t = line.trim();
      if let Some(name) = t.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
        if section.eq_ignore_ascii_case(SECTION) {
          flush(&mut out, &mut written);
        }
        section = name.trim().to_string();
        found |= section.eq_ignore_ascii_case(SECTION);
      } else if section.eq_ignore_ascii_case(SECTION)
        && let Some((k, _)) = t.split_once('=')
        && let Some(i) = values.iter().position(|(n, _)| n.eq_ignore_ascii_case(k.trim()))
      {
        if !written[i] {
          out.push(format!("{}={}", values[i].0, values[i].1));
          written[i] = true;
        }
        continue;
      }
      out.push(line.clone());
    }
    if section.eq_ignore_ascii_case(SECTION) {
      flush(&mut out, &mut written);
    }
    if !found {
      if out.last().is_some_and(|l| !l.trim().is_empty()) {
        out.push(String::new());
      }
      out.push(format!("[{SECTION}]"));
      out.extend(values.iter().map(|(k, v)| format!("{k}={v}")));
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
  }

  pub fn save(&self) -> std::io::Result<()> {
    self.save_to(&Self::path())
  }

  pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
      std::fs::create_dir_all(d)?;
    }
    // write then rename, so a crash never leaves half a file
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, self.to_text())?;
    std::fs::rename(&tmp, path)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_what_the_installer_writes() {
    let s = Settings::parse("\u{feff}[Updates]\r\nMode=notify\r\n");
    assert_eq!(s.update_mode, UpdateMode::Notify);
    assert_eq!(s.last_check, 0);
    assert_eq!(Settings::parse("[updates]\nmode = OFF\nSkip=0.2.0").update_mode, UpdateMode::Off);
    assert_eq!(Settings::parse("[Updates]\nSkip=0.2.0").skip_version.as_deref(), Some("0.2.0"));
    // unknown values fall back to the default
    assert_eq!(Settings::parse("[Updates]\nMode=sometimes").update_mode, UpdateMode::Auto);
    assert_eq!(Settings::parse("[Other]\nMode=off").update_mode, UpdateMode::Auto);
  }

  #[test]
  fn writes_back_keeping_other_lines() {
    let mut s = Settings::parse("; comment\n[Updates]\nMode=notify\nChannel=beta\n\n[Window]\nMaximized=1\n");
    s.last_check = 42;
    s.update_mode = UpdateMode::Off;
    let text = s.to_text();
    assert_eq!(text, "; comment\n[Updates]\nMode=off\nChannel=beta\nLastCheck=42\nSkip=\n\n[Window]\nMaximized=1\n");
    let back = Settings::parse(&text);
    assert_eq!((back.update_mode, back.last_check), (UpdateMode::Off, 42));
    let fresh = Settings::default().to_text();
    assert_eq!(fresh, "[Updates]\nMode=auto\nLastCheck=0\nSkip=\n");
    let dir = std::env::temp_dir().join(format!("fox-settings-{}", std::process::id()));
    let path = dir.join("settings.ini");
    s.save_to(&path).unwrap();
    assert_eq!(Settings::load_from(&path).update_mode, UpdateMode::Off);
    std::fs::remove_dir_all(dir).unwrap();
  }
}
