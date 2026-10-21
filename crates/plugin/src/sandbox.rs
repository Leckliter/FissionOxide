//! What a plugin's engine commands may do: commands that replace the session or its history are
//! refused, and every file path a command takes is checked against the plugin's permissions and
//! rewritten into the folder picked for the run (or the plugin's own `resources/`).

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::manifest::Permission;

/// Commands a plugin can never run: they replace the design, save over the user's file, or
/// change the undo history the host keeps for the run.
const REFUSED: [&str; 9] =
  ["new", "open", "save", "undo", "redo", "transaction.begin", "transaction.commit", "transaction.rollback", "hub.open"];

#[derive(Clone, Copy, PartialEq)]
enum Access {
  Read,
  Write,
}

/// Commands that take file paths: (command, JSON pointer, read or write).
const FILES: [(&str, &str, Access); 17] = [
  ("import", "/path", Access::Read),
  ("inspect", "/path", Access::Read),
  ("form.insert_tsm", "/path", Access::Read),
  ("sketch.insert", "/from/path", Access::Read),
  ("sketch.trace", "/image", Access::Read),
  ("canvas.insert", "/image", Access::Read),
  ("sheet.check_dxf", "/path", Access::Read),
  ("sketch.text", "/font", Access::Read),
  ("sketch.text_edit", "/font", Access::Read),
  ("hub.upload", "/path", Access::Read),
  ("export", "/path", Access::Write),
  ("view.render", "/path", Access::Write),
  ("cam.post", "/path", Access::Write),
  ("drawing.export", "/path", Access::Write),
  ("sheet.flat_pattern", "/path", Access::Write),
  ("cut.gcode", "/path", Access::Write),
  ("hub.download", "/path", Access::Write),
];

/// Keys that hold file paths in commands not listed in [`FILES`].
const PATH_KEYS: [&str; 6] = ["path", "image", "file", "dir", "folder", "font"];

/// What one plugin run may touch.
#[derive(Debug, Clone)]
pub struct Context {
  pub granted: Vec<Permission>,
  /// The folder the user picked for the run (files are read from and written to it).
  pub folder: Option<PathBuf>,
  /// The installed plugin's `resources/` folder.
  pub resources: PathBuf,
}

impl Context {
  fn has(&self, p: Permission) -> bool {
    self.granted.contains(&p)
  }

  fn need(&self, p: Permission, what: &str) -> Result<(), String> {
    if self.has(p) {
      Ok(())
    } else {
      Err(format!("{what} needs the '{}' permission, which this plugin was not given", p.id()))
    }
  }

  /// A path inside the run's folder.
  pub fn in_folder(&self, rel: &str, write: bool) -> Result<PathBuf, String> {
    let base = self.folder.as_ref().ok_or("this command uses files: pick a folder for the run first")?;
    let p = confine(base, rel)?;
    if write && let Some(parent) = p.parent() {
      std::fs::create_dir_all(parent).map_err(|e| format!("{rel}: {e}"))?;
    }
    // a symbolic link inside the folder must not lead out of it
    let real_base = base.canonicalize().map_err(|e| format!("{}: {e}", base.display()))?;
    let existing = if p.exists() { Some(p.clone()) } else { p.parent().map(Path::to_path_buf) };
    if let Some(e) = existing
      && let Ok(real) = e.canonicalize()
      && !real.starts_with(&real_base)
    {
      return Err(format!("'{rel}' leads outside the run's folder"));
    }
    Ok(p)
  }

  /// A file in the plugin's `resources/` folder.
  pub fn resource(&self, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.strip_prefix("resources/").unwrap_or(rel);
    confine(&self.resources, rel)
  }

  /// Checks a command a plugin wants to run and rewrites its paths. Returns the parameters to
  /// run it with.
  pub fn prepare(&self, cmd: &str, params: &Value) -> Result<Value, String> {
    if REFUSED.contains(&cmd) || cmd.starts_with("plugin.") {
      return Err(format!("plugins can't run '{cmd}'"));
    }
    if cmd.starts_with("hub.") {
      self.need(Permission::Hub, &format!("'{cmd}'"))?;
    }
    if cmd == "view.render" {
      self.need(Permission::Render, "'view.render'")?;
    }
    let mut params = params.clone();
    let mut known = false;
    for (c, ptr, access) in FILES {
      if c != cmd {
        continue;
      }
      known = true;
      if let Some(v) = params.pointer_mut(ptr)
        && let Some(s) = v.as_str()
      {
        if ptr == "/font" && !looks_like_file(s) {
          continue;
        }
        *v = Value::String(self.path(cmd, s, access)?.to_string_lossy().into_owned());
      }
    }
    if !known {
      self.rewrite_unknown(cmd, &mut params)?;
    }
    Ok(params)
  }

  fn path(&self, cmd: &str, s: &str, access: Access) -> Result<PathBuf, String> {
    match access {
      Access::Read if s.starts_with("resources/") => self.resource(s),
      Access::Read => {
        self.need(Permission::ReadFiles, &format!("reading '{s}' ('{cmd}')"))?;
        self.in_folder(s, false)
      }
      Access::Write => {
        if cmd != "view.render" {
          self.need(Permission::WriteFiles, &format!("writing '{s}' ('{cmd}')"))?;
        }
        self.in_folder(s, true)
      }
    }
  }

  /// A command this table doesn't know: any string under a path key that looks like a file is
  /// treated as a file written into the run's folder.
  fn rewrite_unknown(&self, cmd: &str, v: &mut Value) -> Result<(), String> {
    match v {
      Value::Object(map) => {
        for (k, item) in map.iter_mut() {
          if let (true, Some(s)) = (PATH_KEYS.contains(&k.as_str()), item.as_str())
            && looks_like_file(s)
          {
            *item = Value::String(self.path(cmd, s, Access::Write)?.to_string_lossy().into_owned());
          } else {
            self.rewrite_unknown(cmd, item)?;
          }
        }
      }
      Value::Array(items) => {
        for item in items {
          self.rewrite_unknown(cmd, item)?;
        }
      }
      _ => {}
    }
    Ok(())
  }
}

fn looks_like_file(s: &str) -> bool {
  s.contains('/')
    || s.contains('\\')
    || Path::new(s).extension().is_some_and(|e| (1..=5).contains(&e.len()) && !s.ends_with(' '))
}

/// `rel` joined to `base`, refusing absolute paths and `..`.
pub fn confine(base: &Path, rel: &str) -> Result<PathBuf, String> {
  let p = Path::new(rel);
  if rel.is_empty() || rel.contains('\\') || !p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
  {
    return Err(format!("'{rel}': plugins use paths relative to the run's folder, without '..'"));
  }
  Ok(base.join(p))
}
