//! The FissionOxide data hub: a local counterpart of Fusion's data panel.
//!
//! Designs live in **projects**, optionally inside **folders**. Every save makes a new
//! **version** of an item instead of overwriting it, so the whole history stays available: open
//! any old version, promote it to be the latest again, mark versions as **milestones**, and see an
//! activity feed per project. Deleted items go to a trash first and can be restored. Files that
//! aren't FissionOxide designs (STEP, STL, a Fusion `.f3d` ...) can be uploaded and kept with the same
//! versioning.
//!
//! On disk the hub is a directory:
//!
//! ```text
//! <root>/hub.json                   index: projects, folders, items, versions, activity
//! <root>/items/<item>/v3.fox        version 3 of a design (or v3.<ext> for other files)
//! <root>/items/<item>/v3.png        its thumbnail
//! <root>/items/<item>/v3.orig.step  the file a version was uploaded from, if it was translated
//! <root>/hub.lock                   held while hub.json is rewritten
//! ```
//!
//! (Newer version files carry a tag of the process that wrote them: `v3-k2x9.fox`.)
//!
//! Version files are written once and never modified; `hub.json` is replaced atomically, so a
//! crash mid-save never corrupts earlier versions. The index is re-read on [`Store::open`] and
//! merged with the one on disk on every write, so the app, the CLI and other machines sharing
//! the hub through a sync folder don't lose each other's changes (see [`sync`]). Versions can
//! carry **comments**.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod sync;

pub const HUB_FORMAT_VERSION: u32 = 1;
const INDEX: &str = "hub.json";
/// Activity entries kept per hub.
const MAX_ACTIVITY: usize = 2000;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
  #[error("{0}")]
  NotFound(String),
  #[error("{0}")]
  Invalid(String),
  #[error("{0}")]
  Io(#[from] std::io::Error),
  #[error("hub index: {0}")]
  Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

fn invalid(s: impl Into<String>) -> StoreError {
  StoreError::Invalid(s.into())
}

/// Seconds since the Unix epoch.
pub type Timestamp = u64;

pub fn now() -> Timestamp {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `2026-10-19 14:03` (UTC).
pub fn format_time(t: Timestamp) -> String {
  let days = (t / 86_400) as i64;
  let secs = t % 86_400;
  // civil-from-days (Howard Hinnant)
  let z = days + 719_468;
  let era = z.div_euclid(146_097);
  let doe = z.rem_euclid(146_097);
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let d = doy - (153 * mp + 2) / 5 + 1;
  let m = if mp < 10 { mp + 3 } else { mp - 9 };
  let y = yoe + era * 400 + i64::from(m <= 2);
  format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, (secs / 60) % 60)
}

/// "just now", "5 min ago", "3 h ago", "2 days ago", or the date.
pub fn relative_time(t: Timestamp, now: Timestamp) -> String {
  let d = now.saturating_sub(t);
  match d {
    0..60 => "just now".into(),
    60..3600 => format!("{} min ago", d / 60),
    3600..86_400 => format!("{} h ago", d / 3600),
    86_400..2_592_000 => format!("{} days ago", d / 86_400),
    _ => format_time(t).chars().take(10).collect(),
  }
}

fn user() -> String {
  std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "me".into())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub description: String,
  pub created: Timestamp,
  #[serde(default)]
  pub trashed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Folder {
  pub id: String,
  pub project: String,
  pub parent: Option<String>,
  pub name: String,
  pub created: Timestamp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
  /// A FissionOxide design (`.fox`), openable and versioned by saves.
  Design,
  /// Any other uploaded file, kept as-is.
  File,
}

/// Where a version came from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Origin {
  Saved,
  /// Uploaded from a file (translated into a design when it's an importable format).
  Uploaded {
    file: String,
  },
  /// Copied from an earlier version ("Promote" in Fusion).
  Promoted {
    from: u32,
  },
  /// Copied from another item ("Save as" / "Copy design").
  Copied {
    item: String,
    version: u32,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Version {
  pub number: u32,
  pub created: Timestamp,
  pub author: String,
  #[serde(default)]
  pub description: String,
  /// Milestone name; milestones are protected from deletion.
  #[serde(default)]
  pub milestone: Option<String>,
  pub origin: Origin,
  /// File name inside the item's directory.
  pub file: String,
  pub size: u64,
  #[serde(default)]
  pub thumbnail: Option<String>,
  /// The original uploaded file when it was translated into a design.
  #[serde(default)]
  pub original: Option<String>,
  /// Free-form facts for the version list (feature/body counts, file inspection results ...).
  #[serde(default)]
  pub stats: serde_json::Value,
  /// Review comments on this version, oldest first.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub comments: Vec<Comment>,
}

/// A comment on a version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Comment {
  pub id: String,
  pub author: String,
  pub time: Timestamp,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
  pub id: String,
  pub project: String,
  pub folder: Option<String>,
  pub name: String,
  pub kind: ItemKind,
  /// File extension of the versions (`fox` for designs).
  pub ext: String,
  pub created: Timestamp,
  pub modified: Timestamp,
  #[serde(default)]
  pub trashed: bool,
  pub versions: Vec<Version>,
  #[serde(default)]
  pub last_opened: Option<Timestamp>,
}

impl Item {
  pub fn latest(&self) -> Option<&Version> {
    self.versions.last()
  }

  pub fn version(&self, n: u32) -> Option<&Version> {
    self.versions.iter().find(|v| v.number == n)
  }

  fn next_number(&self) -> u32 {
    self.versions.iter().map(|v| v.number).max().unwrap_or(0) + 1
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Activity {
  pub time: Timestamp,
  pub author: String,
  pub project: String,
  #[serde(default)]
  pub item: Option<String>,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Index {
  format_version: u32,
  next_id: u64,
  projects: Vec<Project>,
  folders: Vec<Folder>,
  items: Vec<Item>,
  activity: Vec<Activity>,
}

impl Default for Index {
  fn default() -> Self {
    Index {
      format_version: HUB_FORMAT_VERSION,
      next_id: 1,
      projects: Vec::new(),
      folders: Vec::new(),
      items: Vec::new(),
      activity: Vec::new(),
    }
  }
}

/// What to record with a new version.
#[derive(Debug, Clone, Default)]
pub struct VersionMeta {
  pub description: String,
  pub milestone: Option<String>,
  pub thumbnail_png: Option<Vec<u8>>,
  pub stats: serde_json::Value,
  /// For translated uploads: the original file's name and bytes.
  pub original: Option<(String, Vec<u8>)>,
}

/// Where a new item goes.
#[derive(Debug, Clone)]
pub struct NewItem<'a> {
  pub project: &'a str,
  pub folder: Option<&'a str>,
  pub name: &'a str,
  pub kind: ItemKind,
  pub ext: &'a str,
}

/// The contents of a project folder (or the project root).
#[derive(Debug, Clone, Default)]
pub struct Listing {
  pub folders: Vec<Folder>,
  pub items: Vec<Item>,
}

/// One difference between two versions of a design.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Change {
  /// added | removed | changed | renamed | moved
  pub kind: &'static str,
  /// feature | parameter | design
  pub what: &'static str,
  pub name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub detail: Option<String>,
}

pub struct Store {
  root: PathBuf,
  index: Index,
  /// The index as last read from or written to disk: the common ancestor when merging.
  base: Index,
  /// This process's tag in new ids and file names.
  tag: String,
  /// Conflict copies merged when the hub was opened.
  merged: usize,
}

impl Store {
  /// The default hub location: `$FOX_DATA_DIR`, else the platform data directory
  /// (`$XDG_DATA_HOME/fissionoxide`, `~/.local/share/fissionoxide`, `%APPDATA%\FissionOxide`,
  /// `~/Library/Application Support/FissionOxide`). A hub left in the directory of the app's
  /// old name (CLFusion) is moved there the first time.
  pub fn default_root() -> PathBuf {
    if let Some(d) = std::env::var_os("FOX_DATA_DIR") {
      return PathBuf::from(d);
    }
    let root = platform_dir("FissionOxide", "fissionoxide");
    let legacy = platform_dir("CLFusion", "clfusion");
    if !root.exists() && legacy.is_dir() && std::fs::rename(&legacy, &root).is_err() {
      return legacy;
    }
    root
  }

  /// Opens (or creates) a hub at `root`, merging in any conflict copies of its index.
  pub fn open(root: &Path) -> Result<Store> {
    std::fs::create_dir_all(root.join("items"))?;
    let index = Self::read_index(root)?.unwrap_or_default();
    let mut st = Store { root: root.to_path_buf(), base: index.clone(), index, tag: sync::writer_tag(), merged: 0 };
    st.merge_conflict_copies()?;
    Ok(st)
  }

  /// `hub.json`, or `None` if there is none yet.
  fn read_index(root: &Path) -> Result<Option<Index>> {
    match std::fs::read_to_string(root.join(INDEX)) {
      Ok(s) => Ok(Some(Self::parse_index(root, &s)?)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  fn parse_index(root: &Path, s: &str) -> Result<Index> {
    let idx: Index = serde_json::from_str(s)?;
    if idx.format_version > HUB_FORMAT_VERSION {
      return Err(invalid(format!(
        "the hub at {} was written by a newer FissionOxide (format {})",
        root.display(),
        idx.format_version
      )));
    }
    Ok(idx)
  }

  /// Folds conflict copies of `hub.json` into the index and removes them once that is saved.
  /// Copies that don't parse (still syncing?) are left for next time.
  fn merge_conflict_copies(&mut self) -> Result<()> {
    let copies = sync::conflict_copies(&self.root);
    let mut used = Vec::new();
    for c in copies {
      let Ok(text) = std::fs::read_to_string(&c) else { continue };
      let Ok(other) = Self::parse_index(&self.root, &text) else { continue };
      // versions already in hub.json keep their numbers; the copy's clashing ones move
      let merged =
        sync::merge(&serde_json::Value::Null, &serde_json::to_value(&other)?, &serde_json::to_value(&self.index)?);
      self.index = serde_json::from_value(merged)?;
      self.index.next_id = self.index.next_id.max(other.next_id);
      used.push(c);
    }
    if used.is_empty() {
      return Ok(());
    }
    if self.persist_inner()? {
      for c in &used {
        let _ = std::fs::remove_file(c);
      }
      self.merged = used.len();
    }
    Ok(())
  }

  /// How many conflict copies of the index were merged when the hub was opened.
  pub fn merged_conflicts(&self) -> usize {
    self.merged
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  fn persist(&mut self) -> Result<()> {
    self.persist_inner().map(|_| ())
  }

  /// Writes the index under the hub lock, first merging in what other processes wrote since it
  /// was read. Without the lock it goes to a conflict copy instead (merged on a later open);
  /// returns whether `hub.json` itself was written.
  fn persist_inner(&mut self) -> Result<bool> {
    let Some(_lock) = sync::lock(&self.root, &self.tag)? else {
      let path = self.root.join(sync::conflict_name(&self.tag));
      std::fs::write(path, serde_json::to_string_pretty(&self.index)?)?;
      return Ok(false);
    };
    if let Some(disk) = Self::read_index(&self.root)?
      && disk != self.base
    {
      let merged = sync::merge(
        &serde_json::to_value(&self.base)?,
        &serde_json::to_value(&self.index)?,
        &serde_json::to_value(&disk)?,
      );
      let mut idx: Index = serde_json::from_value(merged)?;
      idx.next_id = self.index.next_id.max(disk.next_id);
      self.index = idx;
    }
    let tmp = self.root.join(format!("{INDEX}.{}.tmp", self.tag));
    std::fs::write(&tmp, serde_json::to_string_pretty(&self.index)?)?;
    std::fs::rename(&tmp, self.root.join(INDEX))?;
    self.base = self.index.clone();
    Ok(true)
  }

  fn new_id(&mut self, prefix: &str) -> String {
    let id = format!("{prefix}{}-{}", self.index.next_id, self.tag);
    self.index.next_id += 1;
    id
  }

  fn log(&mut self, project: &str, item: Option<&str>, text: String) {
    self.index.activity.push(Activity {
      time: now(),
      author: user(),
      project: project.to_string(),
      item: item.map(str::to_string),
      text,
    });
    let n = self.index.activity.len();
    if n > MAX_ACTIVITY {
      self.index.activity.drain(..n - MAX_ACTIVITY);
    }
  }

  fn clean_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
      return Err(invalid("the name must not be empty"));
    }
    if n.len() > 200 {
      return Err(invalid("the name is too long"));
    }
    Ok(n.to_string())
  }

  // ---- projects ----

  /// Projects, most recently active first (trashed ones excluded).
  pub fn projects(&self) -> Vec<&Project> {
    let mut v: Vec<&Project> = self.index.projects.iter().filter(|p| !p.trashed).collect();
    let last = |p: &Project| {
      self.index.items.iter().filter(|i| i.project == p.id).map(|i| i.modified).max().unwrap_or(p.created)
    };
    v.sort_by_key(|p| std::cmp::Reverse(last(p)));
    v
  }

  pub fn project(&self, id: &str) -> Result<&Project> {
    self.index.projects.iter().find(|p| p.id == id).ok_or_else(|| StoreError::NotFound(format!("no project '{id}'")))
  }

  /// Finds a project by id or (case-insensitive) name.
  pub fn find_project(&self, key: &str) -> Result<&Project> {
    self.project(key).or_else(|_| {
      self
        .index
        .projects
        .iter()
        .find(|p| !p.trashed && p.name.eq_ignore_ascii_case(key.trim()))
        .ok_or_else(|| StoreError::NotFound(format!("no project '{key}'")))
    })
  }

  pub fn create_project(&mut self, name: &str) -> Result<String> {
    let name = Self::clean_name(name)?;
    if self.index.projects.iter().any(|p| !p.trashed && p.name.eq_ignore_ascii_case(&name)) {
      return Err(invalid(format!("a project named '{name}' already exists")));
    }
    let id = self.new_id("p");
    self.index.projects.push(Project {
      id: id.clone(),
      name: name.clone(),
      description: String::new(),
      created: now(),
      trashed: false,
    });
    self.log(&id, None, format!("created project {name}"));
    self.persist()?;
    Ok(id)
  }

  /// The project new designs go to when none is chosen ("Default Project", created on demand).
  pub fn default_project(&mut self) -> Result<String> {
    if let Some(p) = self.projects().first() {
      return Ok(p.id.clone());
    }
    self.create_project("Default Project")
  }

  pub fn rename_project(&mut self, id: &str, name: &str) -> Result<()> {
    let name = Self::clean_name(name)?;
    let p = self
      .index
      .projects
      .iter_mut()
      .find(|p| p.id == id)
      .ok_or_else(|| StoreError::NotFound(format!("no project '{id}'")))?;
    let old = std::mem::replace(&mut p.name, name.clone());
    self.log(id, None, format!("renamed project {old} to {name}"));
    self.persist()
  }

  /// Moves a project and everything in it to the trash.
  pub fn trash_project(&mut self, id: &str) -> Result<()> {
    let p = self
      .index
      .projects
      .iter_mut()
      .find(|p| p.id == id)
      .ok_or_else(|| StoreError::NotFound(format!("no project '{id}'")))?;
    p.trashed = true;
    let name = p.name.clone();
    self.log(id, None, format!("deleted project {name}"));
    self.persist()
  }

  pub fn restore_project(&mut self, id: &str) -> Result<()> {
    let p = self
      .index
      .projects
      .iter_mut()
      .find(|p| p.id == id)
      .ok_or_else(|| StoreError::NotFound(format!("no project '{id}'")))?;
    p.trashed = false;
    let name = p.name.clone();
    self.log(id, None, format!("restored project {name}"));
    self.persist()
  }

  // ---- folders ----

  pub fn folder(&self, id: &str) -> Result<&Folder> {
    self.index.folders.iter().find(|f| f.id == id).ok_or_else(|| StoreError::NotFound(format!("no folder '{id}'")))
  }

  pub fn create_folder(&mut self, project: &str, parent: Option<&str>, name: &str) -> Result<String> {
    let name = Self::clean_name(name)?;
    self.project(project)?;
    if let Some(p) = parent
      && self.folder(p)?.project != project
    {
      return Err(invalid("the parent folder is in another project"));
    }
    if self
      .index
      .folders
      .iter()
      .any(|f| f.project == project && f.parent.as_deref() == parent && f.name.eq_ignore_ascii_case(&name))
    {
      return Err(invalid(format!("a folder named '{name}' already exists here")));
    }
    let id = self.new_id("f");
    self.index.folders.push(Folder {
      id: id.clone(),
      project: project.to_string(),
      parent: parent.map(str::to_string),
      name: name.clone(),
      created: now(),
    });
    self.log(project, None, format!("created folder {name}"));
    self.persist()?;
    Ok(id)
  }

  pub fn rename_folder(&mut self, id: &str, name: &str) -> Result<()> {
    let name = Self::clean_name(name)?;
    let f = self
      .index
      .folders
      .iter_mut()
      .find(|f| f.id == id)
      .ok_or_else(|| StoreError::NotFound(format!("no folder '{id}'")))?;
    let old = std::mem::replace(&mut f.name, name.clone());
    let project = f.project.clone();
    self.log(&project, None, format!("renamed folder {old} to {name}"));
    self.persist()
  }

  /// Deletes an empty folder (trashed items don't count; they move to the project root).
  pub fn delete_folder(&mut self, id: &str) -> Result<()> {
    let f = self.folder(id)?.clone();
    if self.index.folders.iter().any(|c| c.parent.as_deref() == Some(id))
      || self.index.items.iter().any(|i| !i.trashed && i.folder.as_deref() == Some(id))
    {
      return Err(invalid(format!("folder '{}' is not empty", f.name)));
    }
    for i in self.index.items.iter_mut().filter(|i| i.folder.as_deref() == Some(id)) {
      i.folder = None;
    }
    self.index.folders.retain(|x| x.id != id);
    self.log(&f.project, None, format!("deleted folder {}", f.name));
    self.persist()
  }

  /// Folder path from the project root, e.g. `["Brackets", "Rev B"]`.
  pub fn folder_path(&self, id: Option<&str>) -> Vec<&Folder> {
    let mut out = Vec::new();
    let mut cur = id;
    while let Some(c) = cur {
      let Ok(f) = self.folder(c) else { break };
      out.push(f);
      cur = f.parent.as_deref();
      if out.len() > 64 {
        break;
      }
    }
    out.reverse();
    out
  }

  /// Folders and (non-trashed) items directly inside a project folder, sorted by name / most
  /// recently modified.
  pub fn list(&self, project: &str, folder: Option<&str>) -> Result<Listing> {
    self.project(project)?;
    let mut folders: Vec<Folder> =
      self.index.folders.iter().filter(|f| f.project == project && f.parent.as_deref() == folder).cloned().collect();
    folders.sort_by_key(|f| f.name.to_lowercase());
    let mut items: Vec<Item> = self
      .index
      .items
      .iter()
      .filter(|i| i.project == project && i.folder.as_deref() == folder && !i.trashed)
      .cloned()
      .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.modified));
    Ok(Listing { folders, items })
  }

  // ---- items and versions ----

  pub fn item(&self, id: &str) -> Result<&Item> {
    self.index.items.iter().find(|i| i.id == id).ok_or_else(|| StoreError::NotFound(format!("no item '{id}'")))
  }

  fn item_mut(&mut self, id: &str) -> Result<&mut Item> {
    self.index.items.iter_mut().find(|i| i.id == id).ok_or_else(|| StoreError::NotFound(format!("no item '{id}'")))
  }

  pub fn item_dir(&self, id: &str) -> PathBuf {
    self.root.join("items").join(id)
  }

  /// Path of a version's file (`None` = latest).
  pub fn version_path(&self, item: &str, version: Option<u32>) -> Result<PathBuf> {
    let it = self.item(item)?;
    let v = match version {
      Some(n) => it.version(n).ok_or_else(|| StoreError::NotFound(format!("{} has no version {n}", it.name)))?,
      None => it.latest().ok_or_else(|| StoreError::NotFound(format!("{} has no versions", it.name)))?,
    };
    Ok(self.item_dir(item).join(&v.file))
  }

  pub fn read_version(&self, item: &str, version: Option<u32>) -> Result<Vec<u8>> {
    let path = self.version_path(item, version)?;
    std::fs::read(&path).map_err(|e| {
      if e.kind() == std::io::ErrorKind::NotFound {
        // a shared hub's index can arrive through the sync folder before the version file
        StoreError::NotFound(format!(
          "{} isn't in the hub folder (yet): if the hub is shared through a sync folder, it may still be syncing",
          path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
        ))
      } else {
        e.into()
      }
    })
  }

  pub fn thumbnail(&self, item: &str, version: Option<u32>) -> Option<Vec<u8>> {
    let it = self.item(item).ok()?;
    let v = match version {
      Some(n) => it.version(n)?,
      None => it.latest()?,
    };
    std::fs::read(self.item_dir(item).join(v.thumbnail.as_ref()?)).ok()
  }

  /// Creates a new item with `content` as version 1.
  pub fn create_item(
    &mut self,
    at: NewItem<'_>,
    content: &[u8],
    origin: Origin,
    meta: VersionMeta,
  ) -> Result<(String, u32)> {
    let name = Self::clean_name(at.name)?;
    if self.project(at.project)?.trashed {
      return Err(invalid("the project is in the trash"));
    }
    if let Some(f) = at.folder
      && self.folder(f)?.project != at.project
    {
      return Err(invalid("the folder is in another project"));
    }
    let ext = at.ext.trim_start_matches('.').to_ascii_lowercase();
    if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
      return Err(invalid(format!("bad file extension '{ext}'")));
    }
    let id = self.new_id("i");
    let t = now();
    self.index.items.push(Item {
      id: id.clone(),
      project: at.project.to_string(),
      folder: at.folder.map(str::to_string),
      name: name.clone(),
      kind: at.kind,
      ext,
      created: t,
      modified: t,
      trashed: false,
      versions: Vec::new(),
      last_opened: Some(t),
    });
    let what = match &origin {
      Origin::Uploaded { file } => format!("uploaded {file} as {name}"),
      Origin::Copied { .. } => format!("copied design to {name}"),
      _ => format!("created {name}"),
    };
    match self.write_version(&id, content, origin, meta) {
      Ok(n) => {
        self.log(at.project, Some(&id), what);
        self.persist()?;
        Ok((id, n))
      }
      Err(e) => {
        self.index.items.retain(|i| i.id != id);
        Err(e)
      }
    }
  }

  /// Adds a version to an existing item; returns its number.
  pub fn add_version(&mut self, item: &str, content: &[u8], origin: Origin, meta: VersionMeta) -> Result<u32> {
    if self.item(item)?.trashed {
      return Err(invalid("the item is in the trash; restore it first"));
    }
    self.refresh()?;
    let n = self.write_version(item, content, origin, meta)?;
    let (project, name, file) = {
      let it = self.item(item)?;
      (it.project.clone(), it.name.clone(), it.versions.last().map(|v| v.file.clone()))
    };
    self.log(&project, Some(item), format!("saved {name} v{n}"));
    self.persist()?;
    Ok(self.number_of(item, file).unwrap_or(n))
  }

  /// A version's number by its file name: after a merge renumbered a clash, the one it got.
  fn number_of(&self, item: &str, file: Option<String>) -> Option<u32> {
    self.item(item).ok()?.versions.iter().find(|v| Some(&v.file) == file.as_ref()).map(|v| v.number)
  }

  /// Picks up what other processes (or machines) wrote to the hub since it was read.
  pub fn refresh(&mut self) -> Result<()> {
    let Some(disk) = Self::read_index(&self.root)? else { return Ok(()) };
    if disk == self.base {
      return Ok(());
    }
    let merged = sync::merge(
      &serde_json::to_value(&self.base)?,
      &serde_json::to_value(&self.index)?,
      &serde_json::to_value(&disk)?,
    );
    let mut idx: Index = serde_json::from_value(merged)?;
    idx.next_id = self.index.next_id.max(disk.next_id);
    self.index = idx;
    self.base = disk;
    Ok(())
  }

  fn write_version(&mut self, item: &str, content: &[u8], origin: Origin, meta: VersionMeta) -> Result<u32> {
    let dir = self.item_dir(item);
    std::fs::create_dir_all(&dir)?;
    let (mut n, ext) = {
      let it = self.item(item)?;
      (it.next_number(), it.ext.clone())
    };
    // never reuse a number whose file exists (left over from an interrupted save)
    let tag = self.tag.clone();
    while [format!("v{n}.{ext}"), format!("v{n}.png"), format!("v{n}-{tag}.{ext}")].iter().any(|f| dir.join(f).exists())
    {
      n += 1;
    }
    // the tag keeps two machines saving the same item at once from writing the same file
    let stem = format!("v{n}-{tag}");
    let file = format!("{stem}.{ext}");
    write_new(&dir.join(&file), content)?;
    let thumbnail = match &meta.thumbnail_png {
      Some(png) => {
        let t = format!("{stem}.png");
        write_new(&dir.join(&t), png)?;
        Some(t)
      }
      None => None,
    };
    let original = match &meta.original {
      Some((name, bytes)) => {
        let oext: String = Path::new(name)
          .extension()
          .map(|e| e.to_string_lossy().to_ascii_lowercase())
          .unwrap_or_else(|| "bin".into())
          .chars()
          .filter(char::is_ascii_alphanumeric)
          .collect();
        let o = format!("{stem}.orig.{oext}");
        write_new(&dir.join(&o), bytes)?;
        Some(o)
      }
      None => None,
    };
    let t = now();
    let it = self.item_mut(item)?;
    it.versions.push(Version {
      number: n,
      created: t,
      author: user(),
      description: meta.description.trim().to_string(),
      milestone: meta.milestone.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()),
      origin,
      file,
      size: content.len() as u64,
      thumbnail,
      original,
      stats: meta.stats,
      comments: Vec::new(),
    });
    it.modified = t;
    Ok(n)
  }

  /// Makes an earlier version the latest by copying it as a new version ("Promote").
  pub fn promote(&mut self, item: &str, version: u32) -> Result<u32> {
    let (content, thumb, stats) = {
      let it = self.item(item)?;
      let v =
        it.version(version).ok_or_else(|| StoreError::NotFound(format!("{} has no version {version}", it.name)))?;
      (self.read_version(item, Some(version))?, self.thumbnail(item, Some(version)), v.stats.clone())
    };
    let meta = VersionMeta {
      description: format!("Promoted from v{version}"),
      thumbnail_png: thumb,
      stats,
      ..VersionMeta::default()
    };
    self.refresh()?;
    let n = self.write_version(item, &content, Origin::Promoted { from: version }, meta)?;
    let (project, name, file) = {
      let it = self.item(item)?;
      (it.project.clone(), it.name.clone(), it.versions.last().map(|v| v.file.clone()))
    };
    self.log(&project, Some(item), format!("promoted {name} v{version} to v{n}"));
    self.persist()?;
    Ok(self.number_of(item, file).unwrap_or(n))
  }

  /// Sets (or with `None`/empty, clears) a version's milestone name.
  pub fn set_milestone(&mut self, item: &str, version: u32, name: Option<&str>) -> Result<()> {
    let name = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    let it = self.item_mut(item)?;
    let item_name = it.name.clone();
    let project = it.project.clone();
    let v = it
      .versions
      .iter_mut()
      .find(|v| v.number == version)
      .ok_or_else(|| StoreError::NotFound(format!("no version {version}")))?;
    v.milestone = name.clone();
    let text = match name {
      Some(m) => format!("marked {item_name} v{version} as milestone \"{m}\""),
      None => format!("cleared the milestone on {item_name} v{version}"),
    };
    self.log(&project, Some(item), text);
    self.persist()
  }

  pub fn set_description(&mut self, item: &str, version: u32, description: &str) -> Result<()> {
    let it = self.item_mut(item)?;
    let v = it
      .versions
      .iter_mut()
      .find(|v| v.number == version)
      .ok_or_else(|| StoreError::NotFound(format!("no version {version}")))?;
    v.description = description.trim().to_string();
    self.persist()
  }

  /// Deletes one version's files. The latest version and milestones can't be deleted.
  pub fn delete_version(&mut self, item: &str, version: u32) -> Result<()> {
    let it = self.item(item)?;
    let v = it.version(version).ok_or_else(|| StoreError::NotFound(format!("{} has no version {version}", it.name)))?;
    if it.latest().map(|l| l.number) == Some(version) {
      return Err(invalid("the latest version can't be deleted (promote another version first)"));
    }
    if v.milestone.is_some() {
      return Err(invalid("milestone versions can't be deleted (clear the milestone first)"));
    }
    let dir = self.item_dir(item);
    for f in std::iter::once(&v.file).chain(v.thumbnail.iter()).chain(v.original.iter()) {
      let _ = std::fs::remove_file(dir.join(f));
    }
    let (project, name) = (it.project.clone(), it.name.clone());
    self.item_mut(item)?.versions.retain(|v| v.number != version);
    self.log(&project, Some(item), format!("deleted {name} v{version}"));
    self.persist()
  }

  pub fn rename_item(&mut self, item: &str, name: &str) -> Result<()> {
    let name = Self::clean_name(name)?;
    let it = self.item_mut(item)?;
    let old = std::mem::replace(&mut it.name, name.clone());
    let project = it.project.clone();
    self.log(&project, Some(item), format!("renamed {old} to {name}"));
    self.persist()
  }

  pub fn move_item(&mut self, item: &str, project: &str, folder: Option<&str>) -> Result<()> {
    self.project(project)?;
    if let Some(f) = folder
      && self.folder(f)?.project != project
    {
      return Err(invalid("the folder is in another project"));
    }
    let to = self.project(project)?.name.clone();
    let it = self.item_mut(item)?;
    it.project = project.to_string();
    it.folder = folder.map(str::to_string);
    let name = it.name.clone();
    self.log(project, Some(item), format!("moved {name} to {to}"));
    self.persist()
  }

  /// Copies an item's version (default latest) to a new item.
  pub fn copy_item(&mut self, item: &str, version: Option<u32>, at: NewItem<'_>) -> Result<(String, u32)> {
    let it = self.item(item)?;
    let v = match version {
      Some(n) => it.version(n),
      None => it.latest(),
    }
    .ok_or_else(|| StoreError::NotFound("no such version".into()))?
    .clone();
    let content = self.read_version(item, Some(v.number))?;
    let thumb = self.thumbnail(item, Some(v.number));
    let meta = VersionMeta {
      description: v.description.clone(),
      thumbnail_png: thumb,
      stats: v.stats.clone(),
      ..VersionMeta::default()
    };
    self.create_item(at, &content, Origin::Copied { item: item.to_string(), version: v.number }, meta)
  }

  /// Records that an item was opened (for the Recent list).
  pub fn touch(&mut self, item: &str) -> Result<()> {
    self.item_mut(item)?.last_opened = Some(now());
    self.persist()
  }

  pub fn trash_item(&mut self, item: &str) -> Result<()> {
    let it = self.item_mut(item)?;
    it.trashed = true;
    let (project, name) = (it.project.clone(), it.name.clone());
    self.log(&project, Some(item), format!("deleted {name}"));
    self.persist()
  }

  pub fn restore_item(&mut self, item: &str) -> Result<()> {
    let it = self.item_mut(item)?;
    it.trashed = false;
    let (project, name, folder) = (it.project.clone(), it.name.clone(), it.folder.clone());
    // the folder may have been deleted since
    if let Some(f) = folder
      && self.folder(&f).is_err()
    {
      self.item_mut(item)?.folder = None;
    }
    if self.project(&project)?.trashed {
      self.restore_project(&project)?;
    }
    self.log(&project, Some(item), format!("restored {name}"));
    self.persist()
  }

  /// Permanently deletes a trashed item and all its versions.
  pub fn purge_item(&mut self, item: &str) -> Result<()> {
    let it = self.item(item)?;
    if !it.trashed {
      return Err(invalid("only items in the trash can be deleted permanently"));
    }
    let (project, name) = (it.project.clone(), it.name.clone());
    let dir = self.item_dir(item);
    if dir.starts_with(self.root.join("items")) && dir != self.root.join("items") {
      let _ = std::fs::remove_dir_all(&dir);
    }
    self.index.items.retain(|i| i.id != item);
    self.log(&project, None, format!("permanently deleted {name}"));
    self.persist()
  }

  /// Trashed items (optionally of one project), and trashed projects.
  pub fn trash(&self, project: Option<&str>) -> (Vec<&Item>, Vec<&Project>) {
    let items = self
      .index
      .items
      .iter()
      .filter(|i| {
        (i.trashed || self.project(&i.project).is_ok_and(|p| p.trashed)) && project.is_none_or(|p| i.project == p)
      })
      .collect();
    let projects = self.index.projects.iter().filter(|p| p.trashed).collect();
    (items, projects)
  }

  /// Most recently opened or saved items across all projects.
  pub fn recent(&self, limit: usize) -> Vec<&Item> {
    let mut v: Vec<&Item> = self.live_items().collect();
    v.sort_by_key(|i| std::cmp::Reverse(i.last_opened.unwrap_or(0).max(i.modified)));
    v.truncate(limit);
    v
  }

  fn live_items(&self) -> impl Iterator<Item = &Item> {
    self.index.items.iter().filter(|i| !i.trashed && self.project(&i.project).is_ok_and(|p| !p.trashed))
  }

  /// Items whose name, version descriptions, milestones or comments contain `query` (case-insensitive).
  pub fn search(&self, query: &str) -> Vec<&Item> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
      return Vec::new();
    }
    let mut v: Vec<&Item> = self
      .live_items()
      .filter(|i| {
        i.name.to_lowercase().contains(&q)
          || i.versions.iter().any(|v| {
            v.description.to_lowercase().contains(&q)
              || v.milestone.as_ref().is_some_and(|m| m.to_lowercase().contains(&q))
              || v.comments.iter().any(|c| c.text.to_lowercase().contains(&q))
          })
      })
      .collect();
    v.sort_by_key(|i| std::cmp::Reverse(i.modified));
    v
  }

  /// Newest-first activity, optionally for one project or item.
  pub fn activity(&self, project: Option<&str>, item: Option<&str>, limit: usize) -> Vec<&Activity> {
    self
      .index
      .activity
      .iter()
      .rev()
      .filter(|a| project.is_none_or(|p| a.project == p) && item.is_none_or(|i| a.item.as_deref() == Some(i)))
      .take(limit)
      .collect()
  }

  /// Total bytes used by version files.
  pub fn disk_usage(&self) -> u64 {
    self.index.items.iter().flat_map(|i| i.versions.iter()).map(|v| v.size).sum()
  }
  // ---- comments ----

  /// Adds a comment to a version; returns its id.
  pub fn add_comment(&mut self, item: &str, version: u32, text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
      return Err(invalid("the comment is empty"));
    }
    if text.len() > 10_000 {
      return Err(invalid("the comment is too long"));
    }
    self.item(item)?.version(version).ok_or_else(|| StoreError::NotFound(format!("no version {version}")))?;
    let id = self.new_id("c");
    let it = self.item_mut(item)?;
    let (project, name) = (it.project.clone(), it.name.clone());
    if let Some(v) = it.versions.iter_mut().find(|v| v.number == version) {
      v.comments.push(Comment { id: id.clone(), author: user(), time: now(), text: text.to_string() });
    }
    let short: String = text.chars().take(60).collect();
    let more = if short.len() < text.len() { "…" } else { "" };
    self.log(&project, Some(item), format!("commented on {name} v{version}: \"{short}{more}\""));
    self.persist()?;
    Ok(id)
  }

  /// Comments on an item's versions (`version` = one version only), oldest first, with the
  /// version number each belongs to.
  pub fn comments(&self, item: &str, version: Option<u32>) -> Result<Vec<(u32, &Comment)>> {
    let it = self.item(item)?;
    if let Some(n) = version {
      it.version(n).ok_or_else(|| StoreError::NotFound(format!("{} has no version {n}", it.name)))?;
    }
    let mut v: Vec<(u32, &Comment)> = it
      .versions
      .iter()
      .filter(|v| version.is_none_or(|n| v.number == n))
      .flat_map(|v| v.comments.iter().map(move |c| (v.number, c)))
      .collect();
    v.sort_by_key(|(_, c)| c.time);
    Ok(v)
  }

  /// Deletes a comment.
  pub fn delete_comment(&mut self, item: &str, comment: &str) -> Result<()> {
    let it = self.item_mut(item)?;
    let before: usize = it.versions.iter().map(|v| v.comments.len()).sum();
    for v in &mut it.versions {
      v.comments.retain(|c| c.id != comment);
    }
    if it.versions.iter().map(|v| v.comments.len()).sum::<usize>() == before {
      return Err(StoreError::NotFound(format!("no comment '{comment}'")));
    }
    let (project, name) = (it.project.clone(), it.name.clone());
    self.log(&project, Some(item), format!("deleted a comment on {name}"));
    self.persist()
  }
}

/// Writes a file that must not exist yet (version files are immutable).
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
  use std::io::Write;
  let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
  f.write_all(bytes)?;
  f.sync_all()?;
  Ok(())
}

/// The platform data directory for an app called `name` (`unix` on Linux and other Unixes).
fn platform_dir(name: &str, unix: &str) -> PathBuf {
  let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
  if cfg!(windows) {
    if let Some(a) = std::env::var_os("APPDATA") {
      return PathBuf::from(a).join(name);
    }
  } else if cfg!(target_os = "macos") {
    if let Some(h) = &home {
      return h.join("Library/Application Support").join(name);
    }
  } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
    return PathBuf::from(x).join(unix);
  }
  home.map(|h| h.join(".local/share").join(unix)).unwrap_or_else(|| PathBuf::from(format!("{unix}-data")))
}

/// Compares two designs (their `.fox` JSON): features added, removed, renamed, edited, reordered;
/// parameters added, removed or changed.
pub fn compare_designs(a: &serde_json::Value, b: &serde_json::Value) -> Vec<Change> {
  use serde_json::Value;
  let mut out = Vec::new();
  let name_of = |v: &Value| v.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
  if name_of(a) != name_of(b) {
    out.push(Change { kind: "renamed", what: "design", name: name_of(b), detail: Some(format!("was {}", name_of(a))) });
  }
  let feats = |v: &Value| -> Vec<Value> { v.get("features").and_then(Value::as_array).cloned().unwrap_or_default() };
  let (fa, fb) = (feats(a), feats(b));
  let id = |f: &Value| f.get("id").cloned().unwrap_or(Value::Null);
  let body = |f: &Value| {
    let mut f = f.clone();
    if let Some(o) = f.as_object_mut() {
      o.remove("name");
      o.remove("id");
    }
    f
  };
  for f in &fb {
    match fa.iter().find(|x| id(x) == id(f)) {
      None => out.push(Change { kind: "added", what: "feature", name: name_of(f), detail: None }),
      Some(old) => {
        if name_of(old) != name_of(f) {
          out.push(Change {
            kind: "renamed",
            what: "feature",
            name: name_of(f),
            detail: Some(format!("was {}", name_of(old))),
          });
        }
        if body(old) != body(f) {
          out.push(Change { kind: "changed", what: "feature", name: name_of(f), detail: None });
        }
      }
    }
  }
  for f in &fa {
    if !fb.iter().any(|x| id(x) == id(f)) {
      out.push(Change { kind: "removed", what: "feature", name: name_of(f), detail: None });
    }
  }
  let order = |fs: &[Value], other: &[Value]| -> Vec<Value> {
    fs.iter().map(id).filter(|i| other.iter().any(|o| id(o) == *i)).collect()
  };
  if order(&fa, &fb) != order(&fb, &fa) {
    out.push(Change { kind: "moved", what: "feature", name: "timeline order".into(), detail: None });
  }
  let params = |v: &Value| -> Vec<Value> { v.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default() };
  let (pa, pb) = (params(a), params(b));
  let expr = |p: &Value| p.get("expr").and_then(Value::as_str).unwrap_or_default().to_string();
  for p in &pb {
    match pa.iter().find(|x| name_of(x) == name_of(p)) {
      None => out.push(Change { kind: "added", what: "parameter", name: name_of(p), detail: Some(expr(p)) }),
      Some(old) if expr(old) != expr(p) => out.push(Change {
        kind: "changed",
        what: "parameter",
        name: name_of(p),
        detail: Some(format!("{} → {}", expr(old), expr(p))),
      }),
      _ => {}
    }
  }
  for p in &pa {
    if !pb.iter().any(|x| name_of(x) == name_of(p)) {
      out.push(Change { kind: "removed", what: "parameter", name: name_of(p), detail: None });
    }
  }
  out
}

#[cfg(test)]
mod tests;
