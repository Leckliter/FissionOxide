//! Every change to a hub as one serializable operation: what a store backed by a hub server
//! sends it ([`Store::apply`] on the server's own store carries it out).

use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

use crate::{ItemKind, NewItem, Origin, Result, Store, VersionMeta};

/// Bytes, as base64 in JSON.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bytes(pub Vec<u8>);

impl From<&[u8]> for Bytes {
  fn from(b: &[u8]) -> Self {
    Bytes(b.to_vec())
  }
}

impl Serialize for Bytes {
  fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
  }
}

impl<'de> Deserialize<'de> for Bytes {
  fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
    let s = String::deserialize(d)?;
    base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).map(Bytes).map_err(serde::de::Error::custom)
  }
}

/// [`VersionMeta`] on the wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Meta {
  #[serde(default)]
  pub description: String,
  #[serde(default)]
  pub milestone: Option<String>,
  #[serde(default)]
  pub thumbnail_png: Option<Bytes>,
  #[serde(default)]
  pub stats: Value,
  #[serde(default)]
  pub original: Option<(String, Bytes)>,
}

impl From<VersionMeta> for Meta {
  fn from(m: VersionMeta) -> Self {
    Meta {
      description: m.description,
      milestone: m.milestone,
      thumbnail_png: m.thumbnail_png.map(Bytes),
      stats: m.stats,
      original: m.original.map(|(n, b)| (n, Bytes(b))),
    }
  }
}

impl From<Meta> for VersionMeta {
  fn from(m: Meta) -> Self {
    VersionMeta {
      description: m.description,
      milestone: m.milestone,
      thumbnail_png: m.thumbnail_png.map(|b| b.0),
      stats: m.stats,
      original: m.original.map(|(n, b)| (n, b.0)),
    }
  }
}

/// One change to a hub. The JSON form is `{"op": "create_project", "name": "Brackets"}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
  CreateProject {
    name: String,
  },
  RenameProject {
    id: String,
    name: String,
  },
  TrashProject {
    id: String,
  },
  RestoreProject {
    id: String,
  },
  CreateFolder {
    project: String,
    parent: Option<String>,
    name: String,
  },
  RenameFolder {
    id: String,
    name: String,
  },
  DeleteFolder {
    id: String,
  },
  CreateItem {
    project: String,
    folder: Option<String>,
    name: String,
    kind: ItemKind,
    ext: String,
    content: Bytes,
    origin: Origin,
    #[serde(default)]
    meta: Meta,
  },
  AddVersion {
    item: String,
    content: Bytes,
    origin: Origin,
    #[serde(default)]
    meta: Meta,
  },
  Promote {
    item: String,
    version: u32,
  },
  SetMilestone {
    item: String,
    version: u32,
    name: Option<String>,
  },
  SetDescription {
    item: String,
    version: u32,
    description: String,
  },
  DeleteVersion {
    item: String,
    version: u32,
  },
  RenameItem {
    item: String,
    name: String,
  },
  MoveItem {
    item: String,
    project: String,
    folder: Option<String>,
  },
  CopyItem {
    item: String,
    version: Option<u32>,
    project: String,
    folder: Option<String>,
    name: String,
    kind: ItemKind,
    ext: String,
  },
  Touch {
    item: String,
  },
  TrashItem {
    item: String,
  },
  RestoreItem {
    item: String,
  },
  PurgeItem {
    item: String,
  },
  AddComment {
    item: String,
    version: u32,
    text: String,
  },
  DeleteComment {
    item: String,
    comment: String,
  },
  TakeLock {
    item: String,
    #[serde(default)]
    steal: bool,
  },
  ReleaseLock {
    item: String,
  },
}

impl Op {
  pub(crate) fn create_item(at: &NewItem<'_>, content: &[u8], origin: Origin, meta: VersionMeta) -> Op {
    Op::CreateItem {
      project: at.project.into(),
      folder: at.folder.map(Into::into),
      name: at.name.into(),
      kind: at.kind,
      ext: at.ext.into(),
      content: content.into(),
      origin,
      meta: meta.into(),
    }
  }

  pub(crate) fn copy_item(item: &str, version: Option<u32>, at: &NewItem<'_>) -> Op {
    Op::CopyItem {
      item: item.into(),
      version,
      project: at.project.into(),
      folder: at.folder.map(Into::into),
      name: at.name.into(),
      kind: at.kind,
      ext: at.ext.into(),
    }
  }

  /// Whether the op changes the hub (all do but [`Op::Touch`], which only marks an item as
  /// recently opened).
  pub fn is_edit(&self) -> bool {
    !matches!(self, Op::Touch { .. })
  }
}

impl Store {
  /// Carries out `op`, returning what the matching method returns, as JSON.
  pub fn apply(&mut self, op: Op) -> Result<Value> {
    let done = |r: Result<()>| r.map(|()| Value::Null);
    match op {
      Op::CreateProject { name } => Ok(json!(self.create_project(&name)?)),
      Op::RenameProject { id, name } => done(self.rename_project(&id, &name)),
      Op::TrashProject { id } => done(self.trash_project(&id)),
      Op::RestoreProject { id } => done(self.restore_project(&id)),
      Op::CreateFolder { project, parent, name } => {
        Ok(json!(self.create_folder(&project, parent.as_deref(), &name)?))
      }
      Op::RenameFolder { id, name } => done(self.rename_folder(&id, &name)),
      Op::DeleteFolder { id } => done(self.delete_folder(&id)),
      Op::CreateItem { project, folder, name, kind, ext, content, origin, meta } => {
        let at = NewItem { project: &project, folder: folder.as_deref(), name: &name, kind, ext: &ext };
        Ok(json!(self.create_item(at, &content.0, origin, meta.into())?))
      }
      Op::AddVersion { item, content, origin, meta } => {
        Ok(json!(self.add_version(&item, &content.0, origin, meta.into())?))
      }
      Op::Promote { item, version } => Ok(json!(self.promote(&item, version)?)),
      Op::SetMilestone { item, version, name } => done(self.set_milestone(&item, version, name.as_deref())),
      Op::SetDescription { item, version, description } => done(self.set_description(&item, version, &description)),
      Op::DeleteVersion { item, version } => done(self.delete_version(&item, version)),
      Op::RenameItem { item, name } => done(self.rename_item(&item, &name)),
      Op::MoveItem { item, project, folder } => done(self.move_item(&item, &project, folder.as_deref())),
      Op::CopyItem { item, version, project, folder, name, kind, ext } => {
        let at = NewItem { project: &project, folder: folder.as_deref(), name: &name, kind, ext: &ext };
        Ok(json!(self.copy_item(&item, version, at)?))
      }
      Op::Touch { item } => done(self.touch(&item)),
      Op::TrashItem { item } => done(self.trash_item(&item)),
      Op::RestoreItem { item } => done(self.restore_item(&item)),
      Op::PurgeItem { item } => done(self.purge_item(&item)),
      Op::AddComment { item, version, text } => Ok(json!(self.add_comment(&item, version, &text)?)),
      Op::DeleteComment { item, comment } => done(self.delete_comment(&item, &comment)),
      Op::TakeLock { item, steal } => Ok(serde_json::to_value(self.take_lock(&item, steal)?)?),
      Op::ReleaseLock { item } => done(self.release_lock(&item)),
    }
  }
}
