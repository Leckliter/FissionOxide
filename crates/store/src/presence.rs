//! Presence: who has which design open right now.
//!
//! Each session (an app window, a CLI run) says which designs it has open with
//! [`Store::here`], and repeats it every [`HEARTBEAT`] while they stay open. On a hub server
//! that goes over the WebSocket and everyone sees it at once (a session that disconnects is
//! gone at once too); on a local hub, shared through a sync folder or not, each session keeps a
//! small file in `<root>/presence/`, and one not renewed for [`TTL`] no longer counts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, Store, Timestamp, now};

/// How often a session repeats what it has open.
pub const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(30);
/// Presence not renewed for this long (seconds) has ended: the session quit or crashed.
pub const TTL: Timestamp = 90;
const DIR: &str = "presence";

/// Someone with a design open.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Presence {
  pub user: String,
  pub session: String,
  #[serde(default)]
  pub host: String,
  pub item: String,
  /// When they opened it.
  pub since: Timestamp,
}

/// A session's presence file (and what a hub server is told).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Here {
  /// (A hub server puts in the user of the token instead.)
  #[serde(default)]
  pub user: String,
  pub session: String,
  #[serde(default)]
  pub host: String,
  /// When it was last renewed.
  #[serde(default)]
  pub time: Timestamp,
  /// Open designs and since when.
  #[serde(default)]
  pub items: Vec<(String, Timestamp)>,
}

impl Here {
  /// `items` open now, keeping the opening time of those that were already open in `before`.
  pub fn update(before: Option<&Here>, user: &str, session: &str, host: &str, items: &[String]) -> Here {
    let t = now();
    let since = |i: &String| before.and_then(|b| b.items.iter().find(|(x, _)| x == i)).map(|(_, s)| *s).unwrap_or(t);
    let mut seen = Vec::new();
    for i in items {
      if !seen.iter().any(|(x, _): &(String, Timestamp)| x == i) {
        seen.push((i.clone(), since(i)));
      }
    }
    Here { user: user.into(), session: session.into(), host: host.into(), time: t, items: seen }
  }

  pub fn entries(&self) -> impl Iterator<Item = Presence> + '_ {
    self.items.iter().map(|(item, since)| Presence {
      user: self.user.clone(),
      session: self.session.clone(),
      host: self.host.clone(),
      item: item.clone(),
      since: *since,
    })
  }
}

fn file_of(root: &Path, session: &str) -> PathBuf {
  let safe: String = session.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
  root.join(DIR).join(format!("{safe}.json"))
}

fn read(path: &Path) -> Option<Here> {
  serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

impl Store {
  /// Says which designs this session has open, replacing what it said before (empty: none).
  /// Repeat it every [`HEARTBEAT`] while they stay open.
  pub fn here(&mut self, items: &[String]) -> Result<()> {
    if let Some(conn) = &self.remote {
      return conn.here(items);
    }
    let path = file_of(&self.root, &self.session);
    if items.is_empty() {
      match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => return Ok(()),
      }
    }
    let here = Here::update(read(&path).as_ref(), &self.user, &self.session, &self.host, items);
    std::fs::create_dir_all(self.root.join(DIR))?;
    let tmp = path.with_extension(format!("{}.tmp", self.tag));
    std::fs::write(&tmp, serde_json::to_string(&here)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
  }

  /// Who has designs open (`item` = that design only), this session included, longest open first.
  pub fn presence(&self, item: Option<&str>) -> Result<Vec<Presence>> {
    let mut all = match &self.remote {
      Some(conn) => conn.presence()?,
      None => self.local_presence(),
    };
    all.retain(|p| item.is_none_or(|i| p.item == i));
    all.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.user.cmp(&b.user)));
    Ok(all)
  }

  fn local_presence(&self) -> Vec<Presence> {
    let t = now();
    let Ok(dir) = std::fs::read_dir(self.root.join(DIR)) else { return Vec::new() };
    let mut out = Vec::new();
    for e in dir.flatten() {
      let path = e.path();
      if path.extension().is_none_or(|x| x != "json") {
        continue;
      }
      let Some(h) = read(&path) else { continue };
      let age = t.saturating_sub(h.time);
      if age > TTL {
        // long gone: tidy up after a crashed session
        if age > 86_400 {
          let _ = std::fs::remove_file(&path);
        }
        continue;
      }
      out.extend(h.entries());
    }
    out
  }
}
