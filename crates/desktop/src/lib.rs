//! Desktop integration for the FissionOxide app: where its settings and caches live, updates
//! from GitHub releases (check, download, verify, hand over to the installer) and, on Windows, the
//! notification-area (tray) icon.
//!
//! Nothing here touches the CAD engine; the app's UI drives it (see `fox-ui`'s `updates` module).

pub mod settings;
#[cfg(windows)]
pub mod tray;
pub mod update;
pub mod version;

use std::path::PathBuf;

/// The version of this build (the workspace version, which release tags must match).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Per-user, per-machine app settings and caches: `%LOCALAPPDATA%\FissionOxide` on Windows,
/// `$XDG_CONFIG_HOME/fissionoxide` (`~/.config/fissionoxide`) on Linux,
/// `~/Library/Preferences/FissionOxide` on macOS; `$FOX_CONFIG_DIR` moves it. Kept apart from the
/// data hub, which may be a sync folder shared between computers.
pub fn config_dir() -> PathBuf {
  if let Some(d) = std::env::var_os("FOX_CONFIG_DIR").filter(|d| !d.is_empty()) {
    return PathBuf::from(d);
  }
  let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
  if cfg!(windows) {
    if let Some(a) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
      return PathBuf::from(a).join("FissionOxide");
    }
  } else if cfg!(target_os = "macos") {
    if let Some(h) = &home {
      return h.join("Library/Preferences/FissionOxide");
    }
  } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|x| !x.is_empty()) {
    return PathBuf::from(x).join("fissionoxide");
  }
  home.map(|h| h.join(".config/fissionoxide")).unwrap_or_else(|| PathBuf::from("fissionoxide-config"))
}

/// Opens a web page or folder with the system's default handler.
pub fn open(target: &str) -> std::io::Result<()> {
  let mut cmd = if cfg!(windows) {
    // `start` takes its first quoted argument as the window title
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", "start", "", target]);
    c
  } else if cfg!(target_os = "macos") {
    let mut c = std::process::Command::new("open");
    c.arg(target);
    c
  } else {
    let mut c = std::process::Command::new("xdg-open");
    c.arg(target);
    c
  };
  cmd.spawn().map(|_| ())
}
