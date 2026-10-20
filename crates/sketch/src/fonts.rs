//! Fonts for sketch text: the two built in ("sans", "mono") and TrueType / OpenType files from
//! disk, named by path or by file name (without extension) in the system's font folders.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::SketchError;

/// Largest font file read (bytes).
const MAX_FONT_BYTES: u64 = 64 << 20;
/// How deep font folders are searched.
const MAX_DEPTH: usize = 4;

/// A font's bytes: built in, or read from disk (kept for the rest of the run).
pub(crate) enum FontData {
  Builtin(&'static [u8]),
  Disk(Arc<[u8]>),
}

impl FontData {
  pub(crate) fn bytes(&self) -> &[u8] {
    match self {
      FontData::Builtin(b) => b,
      FontData::Disk(b) => b,
    }
  }
}

fn is_font_file(p: &Path) -> bool {
  p.extension()
    .and_then(|e| e.to_str())
    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
}

/// Folders searched for fonts named without a path, in order: `FOX_FONT_DIR` (colon- or
/// semicolon-separated), then the usual per-user and system font folders.
pub fn font_dirs() -> Vec<PathBuf> {
  let mut out: Vec<PathBuf> = Vec::new();
  if let Some(v) = std::env::var_os("FOX_FONT_DIR") {
    out.extend(std::env::split_paths(&v));
  }
  let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
  if let Some(h) = &home {
    out.extend([h.join(".local/share/fonts"), h.join(".fonts"), h.join("Library/Fonts")]);
  }
  if let Some(l) = std::env::var_os("LOCALAPPDATA") {
    out.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
  }
  if let Some(w) = std::env::var_os("WINDIR") {
    out.push(PathBuf::from(w).join("Fonts"));
  }
  out.extend(
    ["/usr/share/fonts", "/usr/local/share/fonts", "/Library/Fonts", "/System/Library/Fonts"].map(PathBuf::from),
  );
  out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
  let Ok(rd) = std::fs::read_dir(dir) else { return };
  let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
  entries.sort();
  for p in entries {
    if p.is_dir() {
      if depth < MAX_DEPTH {
        walk(&p, depth + 1, out);
      }
    } else if is_font_file(&p) {
      out.push(p);
    }
  }
}

/// Font files found in [`font_dirs`]: (name to use as a text's font, path), sorted by name; the
/// first folder wins when two have the same name.
pub fn system_fonts() -> Vec<(String, PathBuf)> {
  let mut files = Vec::new();
  for d in font_dirs() {
    walk(&d, 0, &mut files);
  }
  let mut out: Vec<(String, PathBuf)> = Vec::new();
  for f in files {
    let Some(name) = f.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
    if !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
      out.push((name, f));
    }
  }
  out.sort_by_key(|a| a.0.to_lowercase());
  out
}

fn cache() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
  static CACHE: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn read(path: &Path) -> Result<Arc<[u8]>, SketchError> {
  let what = |e: &dyn std::fmt::Display| {
    SketchError::InvalidValue(format!("the font {} could not be read: {e}", path.display()))
  };
  let len = std::fs::metadata(path).map_err(|e| what(&e))?.len();
  if len > MAX_FONT_BYTES {
    return Err(what(&"the file is too big"));
  }
  Ok(Arc::from(std::fs::read(path).map_err(|e| what(&e))?))
}

/// The bytes of font `font`: "sans" (or empty), "mono", a path to a font file, or the name of
/// one in the system's font folders.
pub(crate) fn font_data(font: &str) -> Result<FontData, SketchError> {
  match font {
    "" | "sans" => return Ok(FontData::Builtin(epaint_default_fonts::UBUNTU_LIGHT)),
    "mono" => return Ok(FontData::Builtin(epaint_default_fonts::HACK_REGULAR)),
    _ => {}
  }
  if let Some(b) = cache().lock().ok().and_then(|c| c.get(font).cloned()) {
    return Ok(FontData::Disk(b));
  }
  let path = Path::new(font);
  let file = if path.is_file() {
    Some(path.to_path_buf())
  } else {
    system_fonts().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(font)).map(|(_, p)| p)
  };
  let Some(file) = file else {
    return Err(SketchError::InvalidValue(format!(
      "unknown font '{font}' (sans, mono, a font file, or a font in the system's font folders)"
    )));
  };
  let b = read(&file)?;
  if let Ok(mut c) = cache().lock() {
    c.insert(font.to_string(), b.clone());
  }
  Ok(FontData::Disk(b))
}

#[cfg(test)]
mod tests {
  use crate::*;

  #[test]
  fn text_takes_fonts_from_disk_by_path_and_by_name() {
    let dir = std::env::temp_dir().join(format!("fox-sketch-fonts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("FoxTestMono.ttf");
    std::fs::write(&file, epaint_default_fonts::HACK_REGULAR).unwrap();
    let path = file.to_str().unwrap().to_string();
    let mut s = Sketch::new();
    let a = s.add_text(PtSpec::At(0.0, 0.0), "Ag", "mono", 5.0, 0.0).unwrap();
    let b = s.add_text(PtSpec::At(0.0, 0.0), "Ag", &path, 5.0, 0.0).unwrap();
    let pts = |s: &Sketch, t| s.text(t).unwrap().local.clone();
    assert_eq!(pts(&s, a), pts(&s, b), "the same glyphs from the same font file");
    assert_eq!(s.text(b).unwrap().font, path);
    // a file that isn't a font is an error, as is a name found nowhere
    let junk = dir.join("junk.ttf");
    std::fs::write(&junk, b"not a font").unwrap();
    assert!(s.add_text(PtSpec::At(0.0, 0.0), "x", junk.to_str().unwrap(), 5.0, 0.0).is_err());
    assert!(s.add_text(PtSpec::At(0.0, 0.0), "x", "no-such-font-anywhere", 5.0, 0.0).is_err());
    let _ = std::fs::remove_dir_all(&dir);
    // listing the system's fonts never fails, whatever the machine has
    let _ = system_fonts();
    assert!(!font_dirs().is_empty());
  }
}
