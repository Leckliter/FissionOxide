//! Design archives.
//!
//! `.foxz` is FissionOxide's packaged design, the counterpart of Fusion's `.f3d`/`.f3z`: a zip with
//! the design, a thumbnail and (optionally) every saved version, so a design and its history can
//! travel as one file.
//!
//! Fusion's own `.f3d`/`.f3z` are zips too, but the geometry inside is Autodesk ShapeManager B-rep
//! in an undocumented binary encoding. [`inspect_fusion`] reads what is open in them (the file
//! list, preview images, and any standard-format files packed inside) so the data panel can file
//! and preview them.

use std::io::{Read, Write};

use serde_json::{Value, json};

use crate::{ExchangeError, Result};

pub const FOXZ_FORMAT_VERSION: u32 = 1;

/// One stored version inside a `.foxz`.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchivedVersion {
  /// Version metadata (number, time, description, milestone ...), as the data store records it.
  pub info: Value,
  /// The design JSON (`.fox`) of that version.
  pub design: String,
  pub thumbnail: Option<Vec<u8>>,
}

/// The contents of a `.foxz`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Archive {
  pub name: String,
  /// The current design JSON (`.fox`).
  pub design: String,
  pub thumbnail: Option<Vec<u8>>,
  /// Earlier versions, oldest first (empty when the archive carries only the current design).
  pub history: Vec<ArchivedVersion>,
}

fn zip_opts() -> zip::write::SimpleFileOptions {
  zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

pub fn write_foxz(a: &Archive) -> Result<Vec<u8>> {
  let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let mut versions = Vec::new();
  for (k, v) in a.history.iter().enumerate() {
    let n = v.info.get("number").and_then(Value::as_u64).unwrap_or(k as u64 + 1);
    let file = format!("history/v{n}.fox");
    w.start_file(&file, zip_opts())?;
    w.write_all(v.design.as_bytes())?;
    let thumb = match &v.thumbnail {
      Some(png) => {
        let t = format!("history/v{n}.png");
        w.start_file(&t, zip_opts())?;
        w.write_all(png)?;
        Some(t)
      }
      None => None,
    };
    versions.push(json!({"info": v.info, "file": file, "thumbnail": thumb}));
  }
  w.start_file("design.fox", zip_opts())?;
  w.write_all(a.design.as_bytes())?;
  if let Some(png) = &a.thumbnail {
    w.start_file("thumbnail.png", zip_opts())?;
    w.write_all(png)?;
  }
  let manifest = json!({
    "format": "foxz",
    "format_version": FOXZ_FORMAT_VERSION,
    "generator": concat!("FissionOxide ", env!("CARGO_PKG_VERSION")),
    "name": a.name,
    "design": "design.fox",
    "thumbnail": a.thumbnail.as_ref().map(|_| "thumbnail.png"),
    "versions": versions,
  });
  w.start_file("manifest.json", zip_opts())?;
  w.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
  Ok(w.finish()?.into_inner())
}

fn read_entry<R: Read + std::io::Seek>(z: &mut zip::ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
  let mut f = z.by_name(name).map_err(|_| ExchangeError::fmt(format!("archive is missing '{name}'")))?;
  // archives come from other machines: refuse zip bombs
  if f.size() > 1 << 30 {
    return Err(ExchangeError::fmt(format!("'{name}' is too large")));
  }
  let mut v = Vec::with_capacity(f.size() as usize);
  f.read_to_end(&mut v)?;
  Ok(v)
}

pub fn read_foxz(bytes: &[u8]) -> Result<Archive> {
  let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
  let manifest: Value = serde_json::from_slice(&read_entry(&mut z, "manifest.json")?)?;
  // "clfz" archives come from before the rename to FissionOxide and read the same
  if !matches!(manifest.get("format").and_then(Value::as_str), Some("foxz" | "clfz")) {
    return Err(ExchangeError::fmt("not a FissionOxide archive"));
  }
  let fv = manifest.get("format_version").and_then(Value::as_u64).unwrap_or(0);
  if fv > FOXZ_FORMAT_VERSION as u64 {
    return Err(ExchangeError::fmt(format!("archive format {fv} is newer than this FissionOxide supports")));
  }
  let text = |b: Vec<u8>| String::from_utf8(b).map_err(|_| ExchangeError::fmt("design is not UTF-8"));
  let design_name = manifest.get("design").and_then(Value::as_str).unwrap_or("design.fox").to_string();
  let design = text(read_entry(&mut z, &design_name)?)?;
  let thumbnail = match manifest.get("thumbnail").and_then(Value::as_str) {
    Some(t) => Some(read_entry(&mut z, t)?),
    None => None,
  };
  let mut history = Vec::new();
  for v in manifest.get("versions").and_then(Value::as_array).cloned().unwrap_or_default() {
    let file = v.get("file").and_then(Value::as_str).ok_or_else(|| ExchangeError::fmt("version without a file"))?;
    let design = text(read_entry(&mut z, file)?)?;
    let thumbnail = match v.get("thumbnail").and_then(Value::as_str) {
      Some(t) => Some(read_entry(&mut z, t)?),
      None => None,
    };
    history.push(ArchivedVersion { info: v.get("info").cloned().unwrap_or(Value::Null), design, thumbnail });
  }
  let name = manifest.get("name").and_then(Value::as_str).unwrap_or("Untitled").to_string();
  Ok(Archive { name, design, thumbnail, history })
}

/// What could be read from a Fusion `.f3d`/`.f3z`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FusionArchive {
  /// Every file in the archive (nested `.f3d`s of an `.f3z` are listed as `outer/inner:path`).
  pub entries: Vec<(String, u64)>,
  /// The largest preview image found (PNG), used as the thumbnail.
  pub thumbnail: Option<Vec<u8>>,
  /// Number of ShapeManager B-rep blobs (bodies' geometry) found.
  pub brep_blobs: usize,
  /// Designs inside: one for an `.f3d`, one per referenced design for an `.f3z`.
  pub designs: Vec<String>,
  /// Standard-format files packed inside (STEP, STL, ...) that can be imported: (name, bytes).
  pub embedded: Vec<(String, Vec<u8>)>,
}

impl FusionArchive {
  pub fn summary(&self) -> Value {
    json!({
      "entries": self.entries.len(),
      "designs": self.designs,
      "brep_blobs": self.brep_blobs,
      "thumbnail": self.thumbnail.is_some(),
      "embedded": self.embedded.iter().map(|e| &e.0).collect::<Vec<_>>(),
    })
  }
}

const EMBEDDABLE: &[&str] = &["step", "stp", "iges", "igs", "stl", "obj", "3mf", "brep"];

/// Reads the open parts of a Fusion archive. Fails only if the file isn't a zip at all.
pub fn inspect_fusion(bytes: &[u8]) -> Result<FusionArchive> {
  let mut out = FusionArchive::default();
  inspect_into(bytes, "", &mut out, 0)?;
  Ok(out)
}

fn inspect_into(bytes: &[u8], prefix: &str, out: &mut FusionArchive, depth: u32) -> Result<()> {
  let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
    .map_err(|_| ExchangeError::fmt("not a Fusion archive (an .f3d/.f3z is a zip file)"))?;
  let mut designs = std::collections::BTreeSet::new();
  let mut thumb_size = out.thumbnail.as_ref().map(Vec::len).unwrap_or(0);
  for i in 0..z.len() {
    let Ok(mut f) = z.by_index(i) else { continue };
    if f.is_dir() {
      continue;
    }
    let Ok(name) = f.name().map(|n| n.to_string()) else { continue };
    let size = f.size();
    out.entries.push((format!("{prefix}{name}"), size));
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or_default().to_string();
    // Fusion keeps each design's data under a top-level folder named after the design
    if let Some((top, _)) = name.split_once('/') {
      let top = top.trim_end_matches("[Active]").trim();
      if !top.is_empty() && !top.eq_ignore_ascii_case("__MACOSX") {
        designs.insert(top.to_string());
      }
    }
    if lower.contains("brep") || ext == "smt" || ext == "smb" || ext == "smbh" || ext == "sab" {
      out.brep_blobs += 1;
    }
    let want = (ext == "png" && size as usize > thumb_size)
      || EMBEDDABLE.contains(&ext.as_str())
      || (ext == "f3d" && depth == 0);
    if !want || size > 256 << 20 {
      continue;
    }
    let mut data = Vec::with_capacity(size as usize);
    if f.read_to_end(&mut data).is_err() {
      continue;
    }
    drop(f);
    match ext.as_str() {
      "png" => {
        thumb_size = data.len();
        out.thumbnail = Some(data);
      }
      "f3d" => {
        let before = out.designs.len();
        let _ = inspect_into(&data, &format!("{name}:"), out, depth + 1);
        if out.designs.len() == before {
          out.designs.push(name.trim_end_matches(".f3d").to_string());
        }
      }
      _ => out.embedded.push((name, data)),
    }
  }
  out.designs.extend(designs);
  Ok(())
}
