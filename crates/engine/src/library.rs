//! The sketch library: DXF, SVG and `.fox` parts dropped into a sketch where you click
//! (`sketch.library.list`, `sketch.library.insert`).
//!
//! Parts live in library roots: the hub's `library/` folder, the folders in `FOX_LIBRARY_PATH`,
//! folders added with [`add_root`] (an add-in's own pack, the app's settings) and any passed to the
//! command as `folders`. In a root, every sub-folder is a pack (its parts may sit in folders of
//! their own) and files directly in the root form a pack named after the root, so a root can be
//! one pack or hold many. A part is inserted with its origin at the clicked point.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fox_sketch::Sketch;
use serde_json::{Value, json};

use crate::commands::{expr_of, opt_str};
use crate::{CmdError, CmdResult, Session};

/// Extensions a library part may have.
const PART_EXTS: [&str; 4] = ["dxf", "svg", "fox", "foxz"];
/// Points a part's thumbnail outline keeps, at most.
const OUTLINE_POINTS: usize = 400;
/// How deep a pack's folders are searched for parts.
const PACK_DEPTH: usize = 4;

static ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Adds a library root (an add-in's pack folder, or a folder from the app's settings) for every
/// session in this process. Adding one twice keeps one.
pub fn add_root(path: impl Into<PathBuf>) {
  let path = path.into();
  let mut roots = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
  if !roots.contains(&path) {
    roots.push(path);
  }
}

/// Removes a root added with [`add_root`].
pub fn remove_root(path: &Path) {
  ROOTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|r| r != path);
}

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "sketch.library.list" => list(s, p),
    "sketch.library.insert" => insert(s, p),
    _ => return None,
  })
}

/// One library part, found but not read.
#[derive(Debug, Clone)]
struct Part {
  pack: String,
  name: String,
  path: PathBuf,
}

impl Part {
  fn id(&self) -> String {
    format!("{}/{}", self.pack, self.name)
  }
}

/// The library roots for a command: the hub's, the environment's, the added ones and `folders`.
fn roots(s: &Session, p: &Value) -> Vec<PathBuf> {
  let mut out = vec![s.hub_root().join("library")];
  if let Some(v) = std::env::var_os("FOX_LIBRARY_PATH") {
    out.extend(std::env::split_paths(&v));
  }
  out.extend(ROOTS.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned());
  for f in p.get("folders").and_then(Value::as_array).into_iter().flatten() {
    if let Some(f) = f.as_str().filter(|f| !f.trim().is_empty()) {
      out.push(PathBuf::from(f));
    }
  }
  let mut seen = Vec::new();
  out.retain(|r| {
    let fresh = !seen.contains(r);
    seen.push(r.clone());
    fresh
  });
  out
}

fn is_part(path: &Path) -> bool {
  path.is_file()
    && path
      .extension()
      .map(|e| e.to_string_lossy().to_ascii_lowercase())
      .is_some_and(|e| PART_EXTS.contains(&e.as_str()))
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
  let mut v: Vec<PathBuf> =
    std::fs::read_dir(dir).into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path())).collect();
  v.sort();
  v
}

fn stem(path: &Path) -> String {
  path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The parts in a pack folder, named by their path in it (`sub/part`).
fn pack_parts(pack: &str, dir: &Path, prefix: &str, depth: usize, out: &mut Vec<Part>) {
  for e in sorted_entries(dir) {
    if is_part(&e) {
      out.push(Part { pack: pack.to_string(), name: format!("{prefix}{}", stem(&e)), path: e });
    } else if e.is_dir() && depth > 1 {
      let sub = format!("{prefix}{}/", e.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
      pack_parts(pack, &e, &sub, depth - 1, out);
    }
  }
}

/// Every part in the roots; a part id already found in an earlier root keeps that one.
fn scan(roots: &[PathBuf]) -> Vec<Part> {
  let mut out: Vec<Part> = Vec::new();
  for root in roots.iter().filter(|r| r.is_dir()) {
    let mut found = Vec::new();
    let own = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Library".into());
    for e in sorted_entries(root) {
      if is_part(&e) {
        found.push(Part { pack: own.clone(), name: stem(&e), path: e });
      } else if e.is_dir() {
        let pack = e.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        pack_parts(&pack, &e, "", PACK_DEPTH, &mut found);
      }
    }
    for part in found {
      if !out.iter().any(|q| q.id() == part.id()) {
        out.push(part);
      }
    }
  }
  out
}

/// A part's curves as a sketch.
fn read_part(s: &Session, part: &Path) -> Result<Sketch, CmdError> {
  Ok(crate::commands::insert_source(s, &json!({"path": part.to_string_lossy()}))?.0)
}

/// A part's outline for a thumbnail or the placing preview: one polyline per curve (sketch mm,
/// part origin at 0, 0), thinned to about [`OUTLINE_POINTS`] points.
fn outline(sk: &Sketch) -> Vec<Vec<[f64; 2]>> {
  let mut lines: Vec<Vec<[f64; 2]>> = Vec::new();
  for c in sk.curves.iter().filter(|c| !c.construction) {
    for seg in sk.curve_segs(c.id) {
      let mut pts = Vec::new();
      seg.flatten(&mut pts);
      pts.push(seg.end());
      lines.push(pts);
    }
  }
  let total: usize = lines.iter().map(Vec::len).sum();
  if total > OUTLINE_POINTS {
    let step = total.div_ceil(OUTLINE_POINTS);
    for l in &mut lines {
      let last = *l.last().unwrap_or(&[0.0, 0.0]);
      let mut thin: Vec<[f64; 2]> = l.iter().step_by(step).copied().collect();
      if thin.last() != Some(&last) {
        thin.push(last);
      }
      *l = thin;
    }
  }
  let round = |v: f64| (v * 1e4).round() / 1e4;
  lines.into_iter().map(|l| l.into_iter().map(|p| [round(p[0]), round(p[1])]).collect()).collect()
}

fn list(s: &mut Session, p: &Value) -> CmdResult {
  let roots = roots(s, p);
  let with_outline = p.get("outline").and_then(Value::as_bool).unwrap_or(true);
  let parts = scan(&roots);
  let mut packs: Vec<(String, usize)> = Vec::new();
  let mut rows = Vec::new();
  for part in &parts {
    match packs.iter_mut().find(|(n, _)| *n == part.pack) {
      Some((_, n)) => *n += 1,
      None => packs.push((part.pack.clone(), 1)),
    }
    let format = part.path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut row = json!({
      "id": part.id(), "pack": part.pack, "name": part.name,
      "path": part.path.to_string_lossy(), "format": format,
    });
    if with_outline {
      match read_part(s, &part.path) {
        Ok(sk) => {
          if let Some((lo, hi)) = sk.bounds() {
            row["bounds"] = json!([lo, hi]);
            row["size"] = json!([hi[0] - lo[0], hi[1] - lo[1]]);
          }
          row["curves"] = json!(sk.curves.len());
          row["outline"] = json!(outline(&sk));
        }
        Err(e) => row["error"] = json!(e.0),
      }
    }
    rows.push(row);
  }
  Ok(json!({
    "roots": roots.iter().map(|r| json!({"path": r.to_string_lossy(), "exists": r.is_dir()})).collect::<Vec<_>>(),
    "packs": packs.iter().map(|(n, c)| json!({"name": n, "parts": c})).collect::<Vec<_>>(),
    "parts": rows,
  }))
}

fn insert(s: &mut Session, p: &Value) -> CmdResult {
  let want =
    opt_str(p, "part").ok_or(CmdError::new("missing 'part' (a part id from sketch.library.list, or a file)"))?;
  let parts = scan(&roots(s, p));
  let path = match parts.iter().find(|q| q.id() == want) {
    Some(q) => q.path.clone(),
    None => {
      // a bare name, if only one pack has it, or a file
      let named: Vec<&Part> = parts.iter().filter(|q| q.name == want).collect();
      match named.as_slice() {
        [one] => one.path.clone(),
        [_, _, ..] => return Err(CmdError::new(format!("several packs have a part \"{want}\": use pack/name"))),
        [] if is_part(Path::new(want)) => PathBuf::from(want),
        [] => return Err(CmdError::new(format!("no library part \"{want}\" (see sketch.library.list)"))),
      }
    }
  };
  let at =
    p.get("at").and_then(Value::as_array).filter(|a| a.len() == 2).ok_or(CmdError::new("'at' must be [x, y]"))?;
  let mut q = json!({
    "from": {"path": path.to_string_lossy()},
    "translate": [expr_of(&at[0])?, expr_of(&at[1])?],
    "center": [0.0, 0.0],
  });
  for k in ["sketch", "angle", "scale"] {
    if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
      q[k] = v.clone();
    }
  }
  let mut out = crate::commands::run(s, "sketch.insert", &q)?;
  out["part"] = json!(path.to_string_lossy());
  Ok(out)
}
