//! `.foxplugin` packages: a zip of `plugin.toml`, `plugin.wasm`, `icons/`, `resources/` and
//! `README.md`. Reading one checks everything that can be checked before installing it.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Component, Path};

use crate::manifest::Manifest;

pub const MANIFEST: &str = "plugin.toml";
pub const MODULE: &str = "plugin.wasm";
pub const README: &str = "README.md";
pub const EXTENSION: &str = "foxplugin";

/// Limits that keep a hostile package from filling the disk or memory.
const MAX_FILES: usize = 4096;
const MAX_TOTAL: u64 = 256 << 20;

/// A package read into memory and checked.
#[derive(Debug, Clone)]
pub struct Package {
  pub manifest: Manifest,
  /// Every file by its path inside the package (`/`-separated).
  pub files: BTreeMap<String, Vec<u8>>,
}

impl Package {
  pub fn wasm(&self) -> &[u8] {
    &self.files[MODULE]
  }

  pub fn readme(&self) -> Option<String> {
    self.files.get(README).map(|b| String::from_utf8_lossy(b).into_owned())
  }

  /// Reads and checks a `.foxplugin` file's bytes.
  pub fn read(bytes: &[u8]) -> Result<Package, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("not a .foxplugin: {e}"))?;
    if zip.len() > MAX_FILES {
      return Err("the package has too many files".into());
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
      let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
      if f.is_dir() {
        continue;
      }
      let name = f.name().map_err(|e| e.to_string())?.to_string();
      check_path(&name)?;
      total += f.size();
      if total > MAX_TOTAL {
        return Err("the package is too large".into());
      }
      let mut data = Vec::with_capacity(f.size() as usize);
      // `take` guards against a size field that lies
      (&mut f).take(MAX_TOTAL + 1).read_to_end(&mut data).map_err(|e| format!("{name}: {e}"))?;
      files.insert(name, data);
    }
    Package::from_files(files)
  }

  /// Checks a set of files as a package.
  pub fn from_files(files: BTreeMap<String, Vec<u8>>) -> Result<Package, String> {
    let text = files.get(MANIFEST).ok_or("the package has no plugin.toml")?;
    let manifest = Manifest::parse(std::str::from_utf8(text).map_err(|_| "plugin.toml is not UTF-8")?)?;
    let wasm = files.get(MODULE).ok_or("the package has no plugin.wasm")?;
    crate::runtime::check_module(wasm)?;
    for c in &manifest.commands {
      if let Some(icon) = &c.icon {
        let ext = Path::new(icon).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if ext.as_deref() != Some("png") {
          return Err(format!("command '{}': the icon must be a .png", c.key()));
        }
        if !files.contains_key(icon.as_str()) {
          return Err(format!("command '{}': the package has no icon '{icon}'", c.key()));
        }
      }
    }
    Ok(Package { manifest, files })
  }

  /// Reads a plugin from a directory laid out like a package (a plugin's source folder, or an
  /// installed plugin).
  pub fn from_dir(dir: &Path) -> Result<Package, String> {
    let mut files = BTreeMap::new();
    for name in [MANIFEST, MODULE, README, "LICENSE", "LICENSE.md", "LICENSE.txt"] {
      if let Ok(data) = std::fs::read(dir.join(name)) {
        files.insert(name.to_string(), data);
      }
    }
    for sub in ["icons", "resources"] {
      collect(dir, &dir.join(sub), &mut files)?;
    }
    Package::from_files(files)
  }

  /// The package as a `.foxplugin` file.
  pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in &self.files {
      out.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
      out.write_all(data).map_err(|e| e.to_string())?;
    }
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
  }

  /// Writes the files under `dir`.
  pub fn unpack(&self, dir: &Path) -> Result<(), String> {
    for (name, data) in &self.files {
      let p = dir.join(name);
      if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
      }
      std::fs::write(&p, data).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
  }
}

/// Reads only the manifest of a package (for listing the hub's plugins without checking the code).
pub fn peek_manifest(bytes: &[u8]) -> Result<Manifest, String> {
  let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("not a .foxplugin: {e}"))?;
  let mut f = zip.by_name(MANIFEST).map_err(|_| "the package has no plugin.toml")?;
  let mut text = String::new();
  (&mut f).take(1 << 20).read_to_string(&mut text).map_err(|e| e.to_string())?;
  Manifest::parse(&text)
}

/// A package path must be relative, `/`-separated and stay inside the package.
fn check_path(name: &str) -> Result<(), String> {
  let ok = !name.is_empty()
    && !name.contains('\\')
    && !name.contains(':')
    && Path::new(name).components().all(|c| matches!(c, Component::Normal(_)));
  if ok { Ok(()) } else { Err(format!("bad file name in the package: '{name}'")) }
}

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
  let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
  let mut entries: Vec<_> = entries.flatten().collect();
  entries.sort_by_key(|e| e.file_name());
  for e in entries {
    let p = e.path();
    let ty = e.file_type().map_err(|e| e.to_string())?;
    if ty.is_dir() {
      collect(root, &p, files)?;
    } else if ty.is_file() {
      let rel = p.strip_prefix(root).map_err(|e| e.to_string())?;
      let name = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
      check_path(&name)?;
      files.insert(name, std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?);
      if files.len() > MAX_FILES {
        return Err("the plugin has too many files".into());
      }
    }
  }
  Ok(())
}
