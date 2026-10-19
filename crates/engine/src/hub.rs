//! `hub.*` commands: the data panel. Projects, folders and designs with full version history
//! (see [`fox_store`]).
//!
//! A session remembers which hub item its design came from ([`HubLink`]), so `hub.save` adds a
//! new version to that item, as Save does in Fusion, instead of asking for a file name.

use std::path::{Path, PathBuf};

use fox_exchange::archive::{Archive, ArchivedVersion, read_foxz, write_foxz};
use fox_exchange::format_for_path;
use fox_model::Design;
use fox_store::{Item, ItemKind, NewItem, Origin, Store, Version, VersionMeta};
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, Session, files};

/// The hub item (and version) the open design belongs to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HubLink {
  pub item: String,
  pub version: u32,
}

impl From<fox_store::StoreError> for CmdError {
  fn from(e: fox_store::StoreError) -> Self {
    CmdError(e.to_string())
  }
}

fn opt<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
  p.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn req<'a>(p: &'a Value, k: &str) -> Result<&'a str, CmdError> {
  opt(p, k).ok_or_else(|| CmdError::new(format!("missing '{k}'")))
}

pub(crate) fn version_of(p: &Value, k: &str) -> Result<Option<u32>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => v
      .as_u64()
      .and_then(|n| u32::try_from(n).ok())
      .map(Some)
      .ok_or_else(|| CmdError::new(format!("'{k}' must be a version number"))),
  }
}

pub(crate) fn req_version(p: &Value, k: &str) -> Result<u32, CmdError> {
  version_of(p, k)?.ok_or_else(|| CmdError::new(format!("missing '{k}'")))
}

/// `item` param, or the item the open design is linked to.
pub(crate) fn item_of(s: &Session, p: &Value) -> Result<String, CmdError> {
  match opt(p, "item") {
    Some(i) => Ok(i.to_string()),
    None => {
      s.hub.as_ref().map(|h| h.item.clone()).ok_or(CmdError::new("missing 'item' (the open design isn't in the hub)"))
    }
  }
}

/// `project` param (id or name), or the default project.
fn project_of(st: &mut Store, p: &Value) -> Result<String, CmdError> {
  match opt(p, "project") {
    Some(k) => Ok(st.find_project(k)?.id.clone()),
    None => Ok(st.default_project()?),
  }
}

pub fn version_json(v: &Version) -> Value {
  json!({
    "number": v.number,
    "created": v.created,
    "time": fox_store::format_time(v.created),
    "author": v.author,
    "description": v.description,
    "milestone": v.milestone,
    "origin": v.origin,
    "size": v.size,
    "thumbnail": v.thumbnail.is_some(),
    "original": v.original,
    "stats": v.stats,
    "comments": v.comments.iter().map(|c| comment_json(v.number, c)).collect::<Vec<_>>(),
  })
}

fn comment_json(version: u32, c: &fox_store::Comment) -> Value {
  json!({"id": c.id, "version": version, "author": c.author, "time": c.time, "time_text": fox_store::format_time(c.time), "text": c.text})
}

pub fn item_json(st: &Store, it: &Item) -> Value {
  let path: Vec<&str> = st.folder_path(it.folder.as_deref()).iter().map(|f| f.name.as_str()).collect();
  json!({
    "id": it.id,
    "name": it.name,
    "kind": it.kind,
    "ext": it.ext,
    "project": it.project,
    "project_name": st.project(&it.project).map(|p| p.name.clone()).ok(),
    "folder": it.folder,
    "path": path,
    "created": it.created,
    "modified": it.modified,
    "modified_time": fox_store::format_time(it.modified),
    "trashed": it.trashed,
    "versions": it.versions.len(),
    "latest": it.latest().map(version_json),
  })
}

fn stats(s: &Session) -> Value {
  let volume: f64 = s.model.bodies.iter().filter_map(|b| b.shape.mass().ok()).map(|m| m.volume).sum();
  json!({
    "features": s.design.features.len(),
    "bodies": s.model.bodies.len(),
    "parameters": s.design.parameters.len(),
    "errors": s.model.errors().len(),
    "volume_mm3": volume,
  })
}

fn meta(s: &Session, p: &Value) -> VersionMeta {
  VersionMeta {
    description: opt(p, "description").unwrap_or_default().to_string(),
    milestone: opt(p, "milestone").map(str::to_string),
    thumbnail_png: files::thumbnail_png(s, 256, 256),
    stats: stats(s),
    original: None,
  }
}

/// Loads a design version from the hub into a fresh session (no link set).
pub(crate) fn session_for(st: &Store, item: &str, version: Option<u32>) -> Result<Session, CmdError> {
  let it = st.item(item)?;
  let path = st.version_path(item, version)?;
  match it.kind {
    ItemKind::Design => {
      let text =
        String::from_utf8(std::fs::read(&path)?).map_err(|_| CmdError::new("the stored design isn't UTF-8"))?;
      Ok(Session::new(Design::from_json(&text)?))
    }
    // other files are translated on the fly, under their real name so the format is known
    ItemKind::File => {
      let mut s = with_named_copy(&path, &it.name, &it.ext, files::open)?;
      s.path = None;
      Ok(s)
    }
  }
}

/// Runs `f` on a temporary copy of `src` named `<name>.<ext>`.
fn with_named_copy<T>(
  src: &Path,
  name: &str,
  ext: &str,
  f: impl FnOnce(&Path) -> Result<T, CmdError>,
) -> Result<T, CmdError> {
  let dir = std::env::temp_dir().join(format!("fox-hub-{}-{}", std::process::id(), fox_store::now()));
  std::fs::create_dir_all(&dir)?;
  let safe: String =
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
  let tmp = dir.join(format!("{safe}.{ext}"));
  std::fs::copy(src, &tmp)?;
  let r = f(&tmp);
  let _ = std::fs::remove_dir_all(&dir);
  r
}

pub fn run(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let root = s.hub_root();
  let mut st = Store::open(&root)?;
  match name {
    "hub.info" => {
      let items: Vec<&Item> = st.recent(usize::MAX);
      Ok(json!({
        "root": root,
        "projects": st.projects().len(),
        "items": items.len(),
        "versions": items.iter().map(|i| i.versions.len()).sum::<usize>(),
        "bytes": st.disk_usage(),
        "linked": s.hub,
        "merged_conflicts": st.merged_conflicts(),
      }))
    }
    "hub.projects" => {
      let list: Vec<Value> = st
        .projects()
        .iter()
        .map(|pr| {
          let n = st.list(&pr.id, None).map(|l| l.items.len() + l.folders.len()).unwrap_or(0);
          json!({"id": pr.id, "name": pr.name, "created": pr.created, "entries": n})
        })
        .collect();
      Ok(json!(list))
    }
    "hub.project.create" => Ok(json!({"project": st.create_project(req(p, "name")?)?})),
    "hub.project.rename" => {
      let id = st.find_project(req(p, "project")?)?.id.clone();
      st.rename_project(&id, req(p, "name")?)?;
      Ok(json!({}))
    }
    "hub.project.delete" => {
      let id = st.find_project(req(p, "project")?)?.id.clone();
      st.trash_project(&id)?;
      Ok(json!({}))
    }
    "hub.project.restore" => {
      st.restore_project(req(p, "project")?)?;
      Ok(json!({}))
    }
    "hub.folder.create" => {
      let project = project_of(&mut st, p)?;
      Ok(json!({"folder": st.create_folder(&project, opt(p, "parent"), req(p, "name")?)?, "project": project}))
    }
    "hub.folder.rename" => {
      st.rename_folder(req(p, "folder")?, req(p, "name")?)?;
      Ok(json!({}))
    }
    "hub.folder.delete" => {
      st.delete_folder(req(p, "folder")?)?;
      Ok(json!({}))
    }
    "hub.list" => {
      let project = project_of(&mut st, p)?;
      let folder = opt(p, "folder");
      let l = st.list(&project, folder)?;
      let path: Vec<Value> = st.folder_path(folder).iter().map(|f| json!({"id": f.id, "name": f.name})).collect();
      Ok(json!({
        "project": project,
        "folder": folder,
        "path": path,
        "folders": l.folders.iter().map(|f| json!({"id": f.id, "name": f.name, "created": f.created})).collect::<Vec<_>>(),
        "items": l.items.iter().map(|i| item_json(&st, i)).collect::<Vec<_>>(),
      }))
    }
    "hub.save" => {
      let as_new = p.get("new").and_then(Value::as_bool).unwrap_or(false);
      // the design takes the item's name, as in Fusion
      if let Some(n) = opt(p, "name") {
        s.design.name = n.trim().to_string();
      }
      let m = meta(s, p);
      let content = s.design.to_json();
      // an explicit `item` (scripts, the CLI) or the item the design came from
      let target = match opt(p, "item") {
        Some(i) => Some(HubLink { item: st.item(i)?.id.clone(), version: 0 }),
        None => s.hub.clone().filter(|_| !as_new),
      };
      let linked = target.filter(|h| st.item(&h.item).is_ok_and(|i| !i.trashed && i.kind == ItemKind::Design));
      let (item, version) = match linked {
        Some(h) => {
          if let Some(n) = opt(p, "name") {
            st.rename_item(&h.item, n)?;
          }
          let v = st.add_version(&h.item, content.as_bytes(), Origin::Saved, m)?;
          (h.item, v)
        }
        None => {
          let project = project_of(&mut st, p)?;
          let name = opt(p, "name").map(str::to_string).unwrap_or_else(|| s.design.name.clone());
          let at =
            NewItem { project: &project, folder: opt(p, "folder"), name: &name, kind: ItemKind::Design, ext: "fox" };
          st.create_item(at, content.as_bytes(), Origin::Saved, m)?
        }
      };
      s.hub = Some(HubLink { item: item.clone(), version });
      s.dirty = false;
      Ok(json!({"item": item, "version": version}))
    }
    "hub.open" => {
      let item = req(p, "item")?.to_string();
      let version = version_of(p, "version")?;
      let mut ns = session_for(&st, &item, version)?;
      let it = st.item(&item)?;
      let v = match version {
        Some(n) => n,
        None => it.latest().map(|v| v.number).unwrap_or(0),
      };
      let latest = it.latest().map(|v| v.number) == Some(v);
      let kind = it.kind;
      // files are translated: saving makes a new design instead of a version of the file
      ns.hub = (kind == ItemKind::Design).then(|| HubLink { item: item.clone(), version: v });
      ns.store_root = s.store_root.clone();
      *s = ns;
      st.touch(&item)?;
      let mut out = json!({"item": item, "version": v, "features": s.design.features.len()});
      if !latest {
        out["warning"] = json!(format!("opened v{v}, an older version; saving makes it the newest version"));
      }
      Ok(out)
    }
    "hub.versions" => {
      let item = item_of(s, p)?;
      let it = st.item(&item)?;
      Ok(
        json!({"item": item_json(&st, it), "versions": it.versions.iter().rev().map(version_json).collect::<Vec<_>>()}),
      )
    }
    "hub.promote" => {
      let item = item_of(s, p)?;
      let n = st.promote(&item, req_version(p, "version")?)?;
      Ok(json!({"item": item, "version": n}))
    }
    "hub.milestone" => {
      let item = item_of(s, p)?;
      st.set_milestone(&item, req_version(p, "version")?, opt(p, "name"))?;
      Ok(json!({}))
    }
    "hub.describe" => {
      let item = item_of(s, p)?;
      st.set_description(
        &item,
        req_version(p, "version")?,
        p.get("description").and_then(Value::as_str).unwrap_or_default(),
      )?;
      Ok(json!({}))
    }
    "hub.version.delete" => {
      let item = item_of(s, p)?;
      st.delete_version(&item, req_version(p, "version")?)?;
      Ok(json!({}))
    }
    "hub.compare" => {
      let item = item_of(s, p)?;
      let it = st.item(&item)?;
      if it.kind != ItemKind::Design {
        return Err(CmdError::new("only designs can be compared"));
      }
      let a = req_version(p, "a")?;
      let b = version_of(p, "b")?.or(it.latest().map(|v| v.number)).unwrap_or(a);
      let load = |n: u32| -> Result<Value, CmdError> { Ok(serde_json::from_slice(&st.read_version(&item, Some(n))?)?) };
      let changes = fox_store::compare_designs(&load(a)?, &load(b)?);
      Ok(json!({"item": item, "a": a, "b": b, "changes": changes}))
    }
    "hub.item.rename" => {
      st.rename_item(&item_of(s, p)?, req(p, "name")?)?;
      Ok(json!({}))
    }
    "hub.item.move" => {
      let item = item_of(s, p)?;
      let project = project_of(&mut st, p)?;
      st.move_item(&item, &project, opt(p, "folder"))?;
      Ok(json!({}))
    }
    "hub.item.copy" => {
      let item = item_of(s, p)?;
      let src = st.item(&item)?.clone();
      let project = match opt(p, "project") {
        Some(_) => project_of(&mut st, p)?,
        None => src.project.clone(),
      };
      let folder = if opt(p, "project").is_some() {
        opt(p, "folder").map(str::to_string)
      } else {
        opt(p, "folder").map(str::to_string).or(src.folder.clone())
      };
      let name = opt(p, "name").map(str::to_string).unwrap_or_else(|| format!("{} (copy)", src.name));
      let at = NewItem { project: &project, folder: folder.as_deref(), name: &name, kind: src.kind, ext: &src.ext };
      let (id, v) = st.copy_item(&item, version_of(p, "version")?, at)?;
      Ok(json!({"item": id, "version": v}))
    }
    "hub.item.delete" => {
      let item = item_of(s, p)?;
      st.trash_item(&item)?;
      if s.hub.as_ref().is_some_and(|h| h.item == item) {
        s.hub = None;
      }
      Ok(json!({}))
    }
    "hub.item.restore" => {
      st.restore_item(req(p, "item")?)?;
      Ok(json!({}))
    }
    "hub.item.purge" => {
      st.purge_item(req(p, "item")?)?;
      Ok(json!({}))
    }
    "hub.trash" => {
      let (items, projects) = st.trash(opt(p, "project"));
      Ok(json!({
        "items": items.iter().map(|i| item_json(&st, i)).collect::<Vec<_>>(),
        "projects": projects.iter().map(|p| json!({"id": p.id, "name": p.name})).collect::<Vec<_>>(),
      }))
    }
    "hub.recent" => {
      let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
      Ok(json!(st.recent(limit).iter().map(|i| item_json(&st, i)).collect::<Vec<_>>()))
    }
    "hub.search" => Ok(json!(st.search(req(p, "query")?).iter().map(|i| item_json(&st, i)).collect::<Vec<_>>())),
    "hub.activity" => {
      let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
      let project = match opt(p, "project") {
        Some(k) => Some(st.find_project(k)?.id.clone()),
        None => None,
      };
      let list: Vec<Value> = st
        .activity(project.as_deref(), opt(p, "item"), limit)
        .iter()
        .map(|a| json!({"time": a.time, "when": fox_store::format_time(a.time), "author": a.author, "project": a.project, "item": a.item, "text": a.text}))
        .collect();
      Ok(json!(list))
    }
    "hub.upload" => upload(s, &mut st, p),
    "hub.download" => download(&st, s, p),
    "hub.comment" => {
      let item = item_of(s, p)?;
      // default: the version the open design came from, else the latest
      let linked = s.hub.as_ref().filter(|h| h.item == item).map(|h| h.version);
      let version = match version_of(p, "version")?.or(linked) {
        Some(v) => v,
        None => st.item(&item)?.latest().map(|v| v.number).ok_or(CmdError::new("the item has no versions"))?,
      };
      let id = st.add_comment(&item, version, req(p, "text")?)?;
      Ok(json!({"item": item, "version": version, "comment": id}))
    }
    "hub.comments" => {
      let item = item_of(s, p)?;
      let list = st.comments(&item, version_of(p, "version")?)?;
      Ok(json!({"item": item, "comments": list.iter().map(|(n, c)| comment_json(*n, c)).collect::<Vec<_>>()}))
    }
    "hub.comment.delete" => {
      let item = item_of(s, p)?;
      st.delete_comment(&item, req(p, "comment")?)?;
      Ok(json!({}))
    }
    "hub.diff" => crate::diff::run(s, &st, p),
    other => Err(CmdError::new(format!("unknown command '{other}' (try 'help')"))),
  }
}

/// Puts a file in the hub. FissionOxide designs and archives become designs (an archive's whole
/// version history is recreated); importable formats are translated into a design that keeps the
/// original file; anything else (e.g. a Fusion `.f3d`) is stored as-is with its preview.
fn upload(s: &Session, st: &mut Store, p: &Value) -> CmdResult {
  let path = PathBuf::from(req(p, "path")?);
  let bytes = std::fs::read(&path).map_err(|e| CmdError::new(format!("{}: {e}", path.display())))?;
  let fmt = format_for_path(&path).ok_or_else(|| CmdError::new(format!("{}: unknown file type", path.display())))?;
  let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
  let stem = path.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| file.clone());
  let name = opt(p, "name").map(str::to_string).unwrap_or(stem);
  let project = project_of(st, p)?;
  let folder = opt(p, "folder");
  let origin = Origin::Uploaded { file: file.clone() };
  fn design_at<'a>(project: &'a str, folder: Option<&'a str>, name: &'a str) -> NewItem<'a> {
    NewItem { project, folder, name, kind: ItemKind::Design, ext: "fox" }
  }
  match fmt.id {
    "fox" => {
      let tmp = files::open(&path)?;
      let m = VersionMeta {
        thumbnail_png: files::thumbnail_png(&tmp, 256, 256),
        stats: stats(&tmp),
        ..VersionMeta::default()
      };
      let (id, v) = st.create_item(design_at(&project, folder, &name), tmp.design.to_json().as_bytes(), origin, m)?;
      Ok(json!({"item": id, "version": v, "kind": "design"}))
    }
    "foxz" => {
      let a = read_foxz(&bytes).map_err(|e| CmdError::new(e.to_string()))?;
      // replay the archived history oldest first, then the current design
      let mut steps: Vec<(String, Option<Vec<u8>>, Value)> =
        a.history.iter().map(|v| (v.design.clone(), v.thumbnail.clone(), v.info.clone())).collect();
      if steps.last().is_none_or(|l| l.0 != a.design) {
        steps.push((a.design.clone(), a.thumbnail.clone(), Value::Null));
      }
      let mut id: Option<String> = None;
      let mut last = 0;
      for (design, thumb, info) in steps {
        Design::from_json(&design)?;
        let m = VersionMeta {
          description: info.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
          milestone: info.get("milestone").and_then(Value::as_str).map(str::to_string),
          thumbnail_png: thumb,
          stats: info.get("stats").cloned().unwrap_or(Value::Null),
          original: None,
        };
        last = match &id {
          None => {
            let (i, v) = st.create_item(design_at(&project, folder, &name), design.as_bytes(), origin.clone(), m)?;
            id = Some(i);
            v
          }
          Some(i) => st.add_version(i, design.as_bytes(), origin.clone(), m)?,
        };
      }
      Ok(json!({"item": id, "version": last, "kind": "design"}))
    }
    _ => match files::open(&path) {
      Ok(tmp) => {
        let m = VersionMeta {
          description: format!("Translated from {file}"),
          thumbnail_png: files::thumbnail_png(&tmp, 256, 256),
          stats: stats(&tmp),
          original: Some((file.clone(), bytes)),
          ..VersionMeta::default()
        };
        let (id, v) = st.create_item(design_at(&project, folder, &name), tmp.design.to_json().as_bytes(), origin, m)?;
        Ok(json!({"item": id, "version": v, "kind": "design", "translated_from": fmt.id}))
      }
      Err(e) => {
        // can't translate (a Fusion archive): keep the file itself, with what can be read of it
        let (thumb, stats) = if fmt.id == "f3d" {
          match fox_exchange::archive::inspect_fusion(&bytes) {
            Ok(info) => (info.thumbnail.clone(), info.summary()),
            Err(_) => (None, Value::Null),
          }
        } else {
          (None, Value::Null)
        };
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_else(|| "bin".into());
        let m = VersionMeta {
          description: format!("Uploaded {file}"),
          thumbnail_png: thumb,
          stats,
          ..VersionMeta::default()
        };
        let at = NewItem { project: &project, folder, name: &name, kind: ItemKind::File, ext: &ext };
        let (id, v) = st.create_item(at, &bytes, origin, m)?;
        let _ = s;
        Ok(
          json!({"item": id, "version": v, "kind": "file", "warning": format!("stored as a file, not translated: {}", e.0)}),
        )
      }
    },
  }
}

/// Writes a version out of the hub. The target's extension picks the format: the item's own
/// type copies the stored file, `.foxz` packs the design with its whole history, and any export
/// format translates (e.g. a stored design to `.step`).
fn download(st: &Store, s: &Session, p: &Value) -> CmdResult {
  let item = item_of(s, p)?;
  let version = version_of(p, "version")?;
  let target = PathBuf::from(req(p, "path")?);
  let it = st.item(&item)?;
  let ext = target.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
  if ext == it.ext {
    std::fs::write(&target, st.read_version(&item, version)?)?;
    return Ok(json!({"path": target}));
  }
  if ext == "foxz" && it.kind == ItemKind::Design {
    let upto = version.or(it.latest().map(|v| v.number)).unwrap_or(0);
    let mut history = Vec::new();
    for v in it.versions.iter().filter(|v| v.number <= upto) {
      let design = String::from_utf8(st.read_version(&item, Some(v.number))?)
        .map_err(|_| CmdError::new("stored design isn't UTF-8"))?;
      history.push(ArchivedVersion { info: version_json(v), design, thumbnail: st.thumbnail(&item, Some(v.number)) });
    }
    let current = history.last().map(|h| h.design.clone()).unwrap_or_default();
    let a = Archive { name: it.name.clone(), design: current, thumbnail: st.thumbnail(&item, Some(upto)), history };
    std::fs::write(&target, write_foxz(&a).map_err(|e| CmdError::new(e.to_string()))?)?;
    return Ok(json!({"path": target, "versions": a.history.len()}));
  }
  let tmp = session_for(st, &item, version)?;
  let mut q = p.clone();
  q["path"] = json!(target);
  files::export(&tmp, &q)
}

/// Re-exports for the UI.
pub fn open_store(s: &Session) -> Result<Store, CmdError> {
  Ok(Store::open(&s.hub_root())?)
}
