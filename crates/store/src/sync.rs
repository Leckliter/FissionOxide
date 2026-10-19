//! Sharing a hub between machines through a sync folder (Dropbox, OneDrive, Syncthing, a network
//! drive ...) or between the app and the CLI on one machine.
//!
//! Three things keep concurrent writers from losing each other's work:
//!
//! - **Unique names.** Item, project and folder ids and version file names carry a short random
//!   tag of the process that made them, so two machines saving at once never write the same file.
//! - **A lock and a merge on every index write.** `hub.lock` is held for the moment `hub.json` is
//!   rewritten; under it the index on disk is re-read and merged with this process's changes
//!   (a three-way merge against the index as it was loaded), so a save from another process since
//!   then is kept, not overwritten.
//! - **Conflict copies.** When the lock can't be had (another machine's lock arrived through the
//!   sync folder), the index is written as `hub.conflict-<tag>-<time>.json` instead. Those, and
//!   the copies sync tools make when two machines changed `hub.json` at once
//!   (`hub (conflicted copy).json`, `hub.sync-conflict-….json`, `hub-HOST.json` …), are merged
//!   into `hub.json` the next time the hub is opened, then removed.

use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::{MAX_ACTIVITY, now};

pub(crate) const LOCK: &str = "hub.lock";
/// A lock older than this was left by a crashed process (or synced in from elsewhere) and is
/// broken.
const STALE_LOCK: Duration = Duration::from_secs(30);
/// How long a writer waits for the lock before writing a conflict copy instead.
const LOCK_WAIT: Duration = Duration::from_secs(2);

/// A short random tag for this process: lower-case letters and digits.
pub(crate) fn writer_tag() -> String {
  let mut h = std::collections::hash_map::RandomState::new().build_hasher();
  h.write_u32(std::process::id());
  h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
  let mut v = h.finish();
  let mut s = String::new();
  for _ in 0..4 {
    s.push(char::from_digit((v % 36) as u32, 36).unwrap_or('0'));
    v /= 36;
  }
  s
}

/// Holds `hub.lock` until dropped.
pub(crate) struct Lock {
  path: PathBuf,
  token: String,
}

impl Drop for Lock {
  fn drop(&mut self) {
    // only remove the lock if it is still ours (a stale-lock breaker may have replaced it)
    if std::fs::read_to_string(&self.path).is_ok_and(|t| t == self.token) {
      let _ = std::fs::remove_file(&self.path);
    }
  }
}

/// Takes the hub lock, waiting briefly; `None` if another writer keeps holding it.
pub(crate) fn lock(root: &Path, tag: &str) -> std::io::Result<Option<Lock>> {
  use std::io::Write;
  let path = root.join(LOCK);
  let host = std::env::var("HOSTNAME").or_else(|_| std::env::var("COMPUTERNAME")).unwrap_or_default();
  let token = format!("{host} pid {} tag {tag} at {}", std::process::id(), now());
  let start = Instant::now();
  loop {
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
      Ok(mut f) => {
        f.write_all(token.as_bytes())?;
        return Ok(Some(Lock { path, token }));
      }
      Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
        let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
        if age.is_some_and(|a| a > STALE_LOCK) {
          let _ = std::fs::remove_file(&path);
          continue;
        }
        if start.elapsed() > LOCK_WAIT {
          return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(20));
      }
      Err(e) => return Err(e),
    }
  }
}

/// Copies of `hub.json` left by sync tools or by writers that couldn't get the lock.
pub(crate) fn conflict_copies(root: &Path) -> Vec<PathBuf> {
  let Ok(dir) = std::fs::read_dir(root) else { return Vec::new() };
  let mut v: Vec<PathBuf> = dir
    .filter_map(|e| e.ok())
    .map(|e| e.path())
    .filter(|p| {
      let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
      p.is_file() && n.starts_with("hub") && n.ends_with(".json") && n != "hub.json"
    })
    .collect();
  v.sort();
  v
}

/// The name of a conflict copy this process writes.
pub(crate) fn conflict_name(tag: &str) -> String {
  format!("hub.conflict-{tag}-{}.json", now())
}

/// Merges two versions of the hub index that both started from `base` (`Null` when unknown, as
/// for a sync tool's conflict copy). Records are matched by `id` (projects, folders, items,
/// comments) or `file` (versions); additions on either side are kept, a deletion only when the
/// other side didn't change the record, and a value both sides changed takes `ours`. Activity
/// is the union of both feeds. Of two versions of an item that got the same number, `theirs`
/// keeps it and `ours` is renumbered after the highest.
pub(crate) fn merge(base: &Value, ours: &Value, theirs: &Value) -> Value {
  let mut v = merge_value(base, ours, theirs, "");
  if let Some(items) = v.get_mut("items").and_then(Value::as_array_mut) {
    for it in items {
      renumber(it);
    }
  }
  v
}

fn key_of(field: &str) -> Option<&'static str> {
  match field {
    "projects" | "folders" | "items" | "comments" => Some("id"),
    "versions" => Some("file"),
    _ => None,
  }
}

fn merge_value(base: &Value, ours: &Value, theirs: &Value, field: &str) -> Value {
  if ours == theirs || theirs == base {
    return ours.clone();
  }
  if ours == base {
    return theirs.clone();
  }
  match (ours, theirs) {
    (Value::Object(o), Value::Object(t)) => {
      let b = base.as_object();
      let mut out = Map::new();
      for k in o.keys().chain(t.keys().filter(|k| !o.contains_key(*k))) {
        let bv = b.and_then(|b| b.get(k)).unwrap_or(&Value::Null);
        let m = merge_value(bv, o.get(k).unwrap_or(&Value::Null), t.get(k).unwrap_or(&Value::Null), k);
        out.insert(k.clone(), m);
      }
      Value::Object(out)
    }
    (Value::Array(o), Value::Array(t)) if field == "activity" => {
      let b = base.as_array().map(Vec::as_slice).unwrap_or(&[]);
      let mut all: Vec<Value> = t.clone();
      for a in o.iter().filter(|a| !b.contains(a)) {
        if !all.contains(a) {
          all.push(a.clone());
        }
      }
      all.sort_by_key(|a| a.get("time").and_then(Value::as_u64).unwrap_or(0));
      let n = all.len();
      if n > MAX_ACTIVITY {
        all.drain(..n - MAX_ACTIVITY);
      }
      Value::Array(all)
    }
    (Value::Array(o), Value::Array(t)) if key_of(field).is_some() => {
      let key = key_of(field).unwrap_or("id");
      let id = |v: &Value| v.get(key).cloned().unwrap_or(Value::Null);
      let find = |list: &[Value], k: &Value| list.iter().find(|x| id(x) == *k).cloned();
      let b = base.as_array().map(Vec::as_slice).unwrap_or(&[]);
      let mut out = Vec::new();
      // their order first, then records only we have
      for tv in t {
        let k = id(tv);
        let bv = find(b, &k);
        match (find(o, &k), &bv) {
          (Some(ov), _) => out.push(merge_value(bv.as_ref().unwrap_or(&Value::Null), &ov, tv, "")),
          // we deleted it: gone, unless they changed it since
          (None, Some(bv)) if bv == tv => {}
          (None, _) => out.push(tv.clone()),
        }
      }
      for ov in o {
        let k = id(ov);
        if find(t, &k).is_some() {
          continue;
        }
        match find(b, &k) {
          // they deleted it: gone, unless we changed it since
          Some(bv) if bv == *ov => {}
          _ => out.push(ov.clone()),
        }
      }
      Value::Array(out)
    }
    // both changed a plain value (or the shapes differ): ours wins
    _ => ours.clone(),
  }
}

/// Renumbers versions two writers gave the same number: the one already in `theirs` (listed
/// first by the merge) keeps it, the other follows the highest number. Then sorts by number.
fn renumber(item: &mut Value) {
  let Some(vs) = item.get_mut("versions").and_then(Value::as_array_mut) else { return };
  let num = |v: &Value| v.get("number").and_then(Value::as_u64).unwrap_or(0);
  let mut max = vs.iter().map(num).max().unwrap_or(0);
  let mut seen = std::collections::HashSet::new();
  for v in vs.iter_mut() {
    if !seen.insert(num(v))
      && let Some(o) = v.as_object_mut()
    {
      max += 1;
      o.insert("number".into(), Value::from(max));
    }
  }
  vs.sort_by_key(num);
}
