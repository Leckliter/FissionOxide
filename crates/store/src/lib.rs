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
//!
//! A design can be **locked** for editing: while one user holds its lock, nobody else can save
//! a version of it (they can steal the lock, which is recorded in the activity feed). Which
//! designs people have open is their **presence** (see [`presence`]).
//!
//! A hub can also live on a **hub server** (`fox-hub-server`): [`Store::open`] with an
//! `http://` or `https://` URL instead of a directory gives a store backed by it (see
//! [`remote`]). Every change goes to the server as one [`ops::Op`], the index is mirrored
//! locally and version files are cached once fetched; presence and index changes arrive over a
//! WebSocket in real time.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod ops;
pub mod presence;
pub mod remote;
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
  /// Someone else holds the design's edit lock.
  #[error("{0}")]
  Locked(String),
  /// The hub server couldn't be reached, refused the request or answered nonsense.
  #[error("{0}")]
  Remote(String),
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

/// The local user's name: the author of what this process saves to a local hub.
pub fn user() -> String {
  std::env::var("FOX_USER")
    .or_else(|_| std::env::var("USER"))
    .or_else(|_| std::env::var("USERNAME"))
    .unwrap_or_else(|_| "me".into())
}

/// This computer's name, as far as it can be told.
pub fn host() -> String {
  std::env::var("HOSTNAME")
    .or_else(|_| std::env::var("COMPUTERNAME"))
    .ok()
    .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
    .map(|h| h.trim().to_string())
    .unwrap_or_default()
}

/// This process's session id: which app window or CLI run has a design open or locked.
pub fn session_id() -> &'static str {
  static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
  ID.get_or_init(|| format!("s{}{}", sync::writer_tag(), sync::writer_tag()))
}

/// Who holds a design's edit lock.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditLock {
  pub user: String,
  /// The session (app window, CLI run) that took it.
  pub session: String,
  #[serde(default)]
  pub host: String,
  pub since: Timestamp,
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
  /// The edit lock, while someone holds it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub lock: Option<EditLock>,
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
pub(crate) struct Index {
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
  /// Who acts through this store: the author of new versions, comments and activity, and the
  /// holder of the locks it takes.
  user: String,
  session: String,
  host: String,
  /// The hub server this store mirrors, if it isn't a local directory.
  remote: Option<std::sync::Arc<remote::Conn>>,
}

impl Store {
  /// The default hub location: `$FOX_DATA_DIR`, else the platform data directory
  /// (`$XDG_DATA_HOME/fissionoxide`, `~/.local/share/fissionoxide`, `%APPDATA%\FissionOxide`,
  /// `~/Library/Application Support/FissionOxide`). A hub left in the directory of the app's
  /// old name (CLFusion) is moved there the first time.
  ///
  /// In a test process (see [`is_test_process`]) it is never the platform directory: it is
  /// `$FOX_DATA_DIR` if that is set to somewhere else, else a scratch hub for the process in the
  /// temp directory, so tests can't fill the user's real hub with their designs.
  pub fn default_root() -> PathBuf {
    let env = std::env::var_os("FOX_DATA_DIR").map(PathBuf::from);
    if is_test_process() {
      return match env {
        Some(d) if !is_user_hub(&d) => d,
        _ => test_process_root(),
      };
    }
    if let Some(d) = env {
      return d;
    }
    let root = platform_dir("FissionOxide", "fissionoxide");
    let legacy = platform_dir("CLFusion", "clfusion");
    if !root.exists() && legacy.is_dir() && std::fs::rename(&legacy, &root).is_err() {
      return legacy;
    }
    root
  }

  /// Opens (or creates) a hub at `root`, merging in any conflict copies of its index.
  ///
  /// Panics in a test process if `root` is the user's real hub: tests must use a scratch hub.
  ///
  /// `root` may also be a hub server's URL (`https://hub.example.com`), see [`remote`].
  pub fn open(root: &Path) -> Result<Store> {
    if remote::is_remote(root) {
      // as given: a token in the URL is used
      return Self::open_remote(&root.to_string_lossy(), None);
    }
    assert!(
      !(is_test_process() && is_user_hub(root)),
      "a test opened the user's real data hub at {}; give it a scratch hub (Store::scratch_root)",
      root.display()
    );
    std::fs::create_dir_all(root.join("items"))?;
    let index = Self::read_index(root)?.unwrap_or_default();
    let mut st = Store {
      root: root.to_path_buf(),
      base: index.clone(),
      index,
      tag: sync::writer_tag(),
      merged: 0,
      user: user(),
      session: session_id().to_string(),
      host: host(),
      remote: None,
    };
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

  /// The hub directory (for a hub server: the local cache of its files).
  pub fn root(&self) -> &Path {
    &self.root
  }

  /// The hub server's URL, if this store is backed by one.
  pub fn server(&self) -> Option<&str> {
    self.remote.as_ref().map(|c| c.url.as_str())
  }

  /// The user this store acts as (on a hub server: the one its token belongs to).
  pub fn user(&self) -> &str {
    &self.user
  }

  /// The session this store acts for (see [`session_id`]).
  pub fn session(&self) -> &str {
    &self.session
  }

  /// Acts as someone else from now on: a hub server sets the user a request's token belongs to,
  /// and the session and host it came from, before applying it.
  pub fn set_actor(&mut self, user: &str, session: &str, host: &str) {
    self.user = user.to_string();
    self.session = session.to_string();
    self.host = host.to_string();
  }

  /// A store backed by the hub server at `url` (`token` = its access token; `None` = the one
  /// remembered for it, see [`remote::token_for`]).
  pub fn open_remote(url: &str, token: Option<&str>) -> Result<Store> {
    let conn = remote::connect(url, token)?;
    let index = conn.index()?;
    Ok(Store {
      root: conn.cache_dir(),
      base: index.clone(),
      index,
      tag: sync::writer_tag(),
      merged: 0,
      user: conn.user.clone(),
      session: session_id().to_string(),
      host: host(),
      remote: Some(conn),
    })
  }

  /// The whole index as JSON, as a hub server sends it to its clients.
  pub fn snapshot(&self) -> serde_json::Value {
    serde_json::to_value(&self.index).unwrap_or(serde_json::Value::Null)
  }

  /// Sends `op` to the hub server and mirrors the index it leaves.
  fn remote_op<T: serde::de::DeserializeOwned>(&mut self, op: ops::Op) -> Result<T> {
    let Some(conn) = self.remote.clone() else { return Err(invalid("not a hub server")) };
    let v = conn.op(&op, &self.session, &self.host);
    // a refusal (someone else's lock ...) usually means the mirror is behind, too
    let fresh = self.refresh();
    let v = v?;
    fresh?;
    serde_json::from_value(v).map_err(|e| StoreError::Remote(format!("the hub server's answer: {e}")))
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
      author: self.user.clone(),
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::CreateProject { name: name.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::RenameProject { id: id.into(), name: name.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::TrashProject { id: id.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::RestoreProject { id: id.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::CreateFolder {
        project: project.into(),
        parent: parent.map(Into::into),
        name: name.into(),
      });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::RenameFolder { id: id.into(), name: name.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::DeleteFolder { id: id.into() });
    }
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
    match &self.remote {
      Some(conn) => conn.cached_file(item, &v.file),
      None => Ok(self.item_dir(item).join(&v.file)),
    }
  }

  /// The file a translated upload came from (`None` = latest version): its stored name and bytes,
  /// or `None` when the version wasn't translated from a file.
  pub fn read_original(&self, item: &str, version: Option<u32>) -> Result<Option<(String, Vec<u8>)>> {
    let it = self.item(item)?;
    let v = match version {
      Some(n) => it.version(n).ok_or_else(|| StoreError::NotFound(format!("{} has no version {n}", it.name)))?,
      None => it.latest().ok_or_else(|| StoreError::NotFound(format!("{} has no versions", it.name)))?,
    };
    let Some(name) = v.original.clone() else { return Ok(None) };
    let path = match &self.remote {
      Some(conn) => conn.cached_file(item, &name)?,
      None => self.item_dir(item).join(&name),
    };
    Ok(Some((name, std::fs::read(path)?)))
  }

  pub fn read_version(&self, item: &str, version: Option<u32>) -> Result<Vec<u8>> {
    let path = self.version_path(item, version)?;
    if self.remote.is_some() {
      return Ok(std::fs::read(&path)?);
    }
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
    let file = v.thumbnail.as_ref()?;
    match &self.remote {
      Some(conn) => std::fs::read(conn.cached_file(item, file).ok()?).ok(),
      None => std::fs::read(self.item_dir(item).join(file)).ok(),
    }
  }

  /// A file of an item's versions by its name in the item's directory (a hub server serves
  /// these): only names the index lists.
  pub fn item_file(&self, item: &str, file: &str) -> Result<Vec<u8>> {
    let it = self.item(item)?;
    let listed = it
      .versions
      .iter()
      .any(|v| v.file == file || v.thumbnail.as_deref() == Some(file) || v.original.as_deref() == Some(file));
    if !listed {
      return Err(StoreError::NotFound(format!("{} has no file '{file}'", it.name)));
    }
    match &self.remote {
      Some(conn) => Ok(std::fs::read(conn.cached_file(item, file)?)?),
      None => Ok(std::fs::read(self.item_dir(item).join(file))?),
    }
  }

  /// Creates a new item with `content` as version 1.
  pub fn create_item(
    &mut self,
    at: NewItem<'_>,
    content: &[u8],
    origin: Origin,
    meta: VersionMeta,
  ) -> Result<(String, u32)> {
    if self.remote.is_some() {
      return self.remote_op(ops::Op::create_item(&at, content, origin, meta));
    }
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
      lock: None,
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::AddVersion {
        item: item.into(),
        content: content.into(),
        origin,
        meta: meta.into(),
      });
    }
    if self.item(item)?.trashed {
      return Err(invalid("the item is in the trash; restore it first"));
    }
    self.refresh()?;
    self.check_unlocked(item)?;
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
    self.reload().map(|_| ())
  }

  /// [`refresh`](Self::refresh), saying whether anything changed.
  pub fn reload(&mut self) -> Result<bool> {
    if let Some(conn) = &self.remote {
      let index = conn.index()?;
      let changed = index != self.index;
      self.base = index.clone();
      self.index = index;
      return Ok(changed);
    }
    let Some(disk) = Self::read_index(&self.root)? else { return Ok(false) };
    if disk == self.base {
      return Ok(false);
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
    Ok(true)
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
    let author = self.user.clone();
    let it = self.item_mut(item)?;
    it.versions.push(Version {
      number: n,
      created: t,
      author,
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::Promote { item: item.into(), version });
    }
    self.check_unlocked(item)?;
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::SetMilestone { item: item.into(), version, name: name.map(Into::into) });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::SetDescription { item: item.into(), version, description: description.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::DeleteVersion { item: item.into(), version });
    }
    self.check_unlocked(item)?;
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::RenameItem { item: item.into(), name: name.into() });
    }
    let name = Self::clean_name(name)?;
    let it = self.item_mut(item)?;
    let old = std::mem::replace(&mut it.name, name.clone());
    let project = it.project.clone();
    self.log(&project, Some(item), format!("renamed {old} to {name}"));
    self.persist()
  }

  pub fn move_item(&mut self, item: &str, project: &str, folder: Option<&str>) -> Result<()> {
    if self.remote.is_some() {
      return self.remote_op(ops::Op::MoveItem {
        item: item.into(),
        project: project.into(),
        folder: folder.map(Into::into),
      });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::copy_item(item, version, &at));
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::Touch { item: item.into() });
    }
    self.item_mut(item)?.last_opened = Some(now());
    self.persist()
  }

  pub fn trash_item(&mut self, item: &str) -> Result<()> {
    if self.remote.is_some() {
      return self.remote_op(ops::Op::TrashItem { item: item.into() });
    }
    self.check_unlocked(item)?;
    let it = self.item_mut(item)?;
    it.trashed = true;
    let (project, name) = (it.project.clone(), it.name.clone());
    self.log(&project, Some(item), format!("deleted {name}"));
    self.persist()
  }

  pub fn restore_item(&mut self, item: &str) -> Result<()> {
    if self.remote.is_some() {
      return self.remote_op(ops::Op::RestoreItem { item: item.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::PurgeItem { item: item.into() });
    }
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::AddComment { item: item.into(), version, text: text.into() });
    }
    let text = text.trim();
    if text.is_empty() {
      return Err(invalid("the comment is empty"));
    }
    if text.len() > 10_000 {
      return Err(invalid("the comment is too long"));
    }
    self.item(item)?.version(version).ok_or_else(|| StoreError::NotFound(format!("no version {version}")))?;
    let id = self.new_id("c");
    let author = self.user.clone();
    let it = self.item_mut(item)?;
    let (project, name) = (it.project.clone(), it.name.clone());
    if let Some(v) = it.versions.iter_mut().find(|v| v.number == version) {
      v.comments.push(Comment { id: id.clone(), author, time: now(), text: text.to_string() });
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
    if self.remote.is_some() {
      return self.remote_op(ops::Op::DeleteComment { item: item.into(), comment: comment.into() });
    }
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

// ---- edit locks ----

impl Store {
  /// The design's edit lock, if someone holds it.
  pub fn lock_of(&self, item: &str) -> Result<Option<&EditLock>> {
    Ok(self.item(item)?.lock.as_ref())
  }

  /// Whether `lock` is held by the user this store acts as.
  pub fn is_mine(&self, lock: &EditLock) -> bool {
    lock.user == self.user
  }

  /// Fails if someone other than this store's user holds the item's lock.
  fn check_unlocked(&self, item: &str) -> Result<()> {
    let it = self.item(item)?;
    match &it.lock {
      Some(l) if !self.is_mine(l) => Err(StoreError::Locked(format!(
        "{} is locked by {}{} since {}: ask them to release it, or steal the lock",
        it.name,
        l.user,
        if l.host.is_empty() { String::new() } else { format!(" on {}", l.host) },
        format_time(l.since)
      ))),
      _ => Ok(()),
    }
  }

  /// Takes a design's edit lock, so nobody else can save versions of it until it is released.
  /// Fails if someone else holds it, unless `steal` (which takes it from them).
  pub fn take_lock(&mut self, item: &str, steal: bool) -> Result<EditLock> {
    if self.remote.is_some() {
      return self.remote_op(ops::Op::TakeLock { item: item.into(), steal });
    }
    self.refresh()?;
    if !steal {
      self.check_unlocked(item)?;
    }
    let lock =
      EditLock { user: self.user.clone(), session: self.session.clone(), host: self.host.clone(), since: now() };
    let it = self.item_mut(item)?;
    if it.kind != ItemKind::Design {
      return Err(invalid(format!("{} is a file, not a design: only designs can be locked", it.name)));
    }
    let (project, name) = (it.project.clone(), it.name.clone());
    let before = it.lock.clone();
    // taken again by its holder: keep when it was first taken
    let lock = match &before {
      Some(b) if b.user == lock.user => EditLock { since: b.since, ..lock },
      _ => lock,
    };
    it.lock = Some(lock.clone());
    match before {
      Some(b) if b.user != lock.user => {
        self.log(&project, Some(item), format!("took the lock on {name} from {}", b.user))
      }
      Some(_) => {}
      None => self.log(&project, Some(item), format!("locked {name} for editing")),
    }
    self.persist()?;
    Ok(lock)
  }

  /// Releases the design's lock. Someone else's lock can't be released (steal it instead);
  /// an unlocked design is left as it is.
  pub fn release_lock(&mut self, item: &str) -> Result<()> {
    if self.remote.is_some() {
      return self.remote_op(ops::Op::ReleaseLock { item: item.into() });
    }
    self.refresh()?;
    let it = self.item(item)?;
    let Some(l) = &it.lock else { return Ok(()) };
    if !self.is_mine(l) {
      return Err(StoreError::Locked(format!("{} is locked by {}; steal the lock to take it over", it.name, l.user)));
    }
    let it = self.item_mut(item)?;
    it.lock = None;
    let (project, name) = (it.project.clone(), it.name.clone());
    self.log(&project, Some(item), format!("released the lock on {name}"));
    self.persist()
  }

  /// Designs this store's user has locked.
  pub fn my_locks(&self) -> Vec<&Item> {
    self.index.items.iter().filter(|i| i.lock.as_ref().is_some_and(|l| self.is_mine(l))).collect()
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

/// Whether this process is a Cargo test executable (unit, integration or bench), from any crate.
/// Cargo builds those into `target/<profile>/deps/`, where the app is never run from: `cargo
/// run` and the installer use the binary copied up to `target/<profile>/` or installed.
/// `cfg(test)` can't tell, as it is only set for the crate under test, not its dependencies.
pub fn is_test_process() -> bool {
  static TEST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
  *TEST.get_or_init(|| {
    std::env::current_exe()
      .ok()
      .and_then(|e| e.parent().and_then(Path::file_name).map(|d| d == "deps"))
      .unwrap_or(false)
  })
}

/// Whether `dir` is the user's real hub (the platform data directory, or its old CLFusion one).
fn is_user_hub(dir: &Path) -> bool {
  let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
  let dir = canon(dir);
  [platform_dir("FissionOxide", "fissionoxide"), platform_dir("CLFusion", "clfusion")].iter().any(|real| {
    let real = canon(real);
    real == dir || (cfg!(windows) && real.to_string_lossy().eq_ignore_ascii_case(&dir.to_string_lossy()))
  })
}

/// Where test processes keep their scratch hubs.
fn test_hubs_dir() -> PathBuf {
  std::env::temp_dir().join("fox-test-hubs")
}

/// The scratch hub a test process falls back to when nothing names a hub: one per process,
/// in [`test_hubs_dir`]. Scratch hubs left by earlier runs more than a day ago are removed.
fn test_process_root() -> PathBuf {
  static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
  ROOT
    .get_or_init(|| {
      let dir = test_hubs_dir();
      let day = std::time::Duration::from_secs(24 * 3600);
      for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a > day);
        if old {
          let _ = std::fs::remove_dir_all(e.path());
        }
      }
      dir.join(format!("{}", std::process::id()))
    })
    .clone()
}

impl Store {
  /// A fresh, empty hub location for a test: a new directory under the temp directory each
  /// call (not created until the hub is first written).
  pub fn scratch_root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    test_process_root().with_extension(n.to_string())
  }
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
