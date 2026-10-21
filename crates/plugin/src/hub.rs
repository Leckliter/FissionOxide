//! Plugins in the data hub: a `.foxplugin` published to the hub is a file item whose versions
//! are the plugin's releases, so anyone sharing the hub can install it and pick up updates.

use fox_engine::store::{ItemKind, NewItem, Origin, Store, VersionMeta};
use serde::Serialize;

use crate::package::{self, Package};
use crate::{InstallOptions, Manifest, Plan, Plugins};

/// A plugin package in the hub (the latest version of its item).
#[derive(Debug, Clone, Serialize)]
pub struct HubPlugin {
  pub item: String,
  pub hub_version: u32,
  pub id: String,
  pub name: String,
  pub version: String,
  pub description: String,
  /// The version installed here, if any.
  pub installed: Option<String>,
  /// Newer than the installed version.
  pub update: bool,
}

/// Every live hub item holding a `.foxplugin`.
fn plugin_items(st: &Store) -> Vec<(String, u32)> {
  let mut out = Vec::new();
  for p in st.projects() {
    let mut folders = vec![None];
    while let Some(f) = folders.pop() {
      let Ok(l) = st.list(&p.id, f.as_deref()) else { continue };
      folders.extend(l.folders.iter().map(|f| Some(f.id.clone())));
      for it in l.items.iter().filter(|i| i.ext == package::EXTENSION && !i.trashed) {
        if let Some(v) = it.latest() {
          out.push((it.id.clone(), v.number));
        }
      }
    }
  }
  out
}

/// The hub's plugins, with whether each is installed and has an update.
pub fn available(plugins: &Plugins, st: &Store) -> Vec<HubPlugin> {
  let mut out = Vec::new();
  for (item, n) in plugin_items(st) {
    let Ok(bytes) = st.read_version(&item, Some(n)) else { continue };
    let Ok(m) = package::peek_manifest(&bytes) else { continue };
    let installed = plugins.get(&m.id).map(|p| p.record.version.clone());
    let update = installed
      .as_ref()
      .is_some_and(|v| semver::Version::parse(v).is_ok_and(|v| m.version() > v));
    out.push(HubPlugin {
      item,
      hub_version: n,
      id: m.id,
      name: m.name,
      version: m.version,
      description: m.description,
      installed,
      update,
    });
  }
  out.sort_by(|a, b| a.name.cmp(&b.name).then(a.item.cmp(&b.item)));
  out
}

/// Installs a hub item's package (its latest version unless one is given).
pub fn install(
  plugins: &mut Plugins,
  st: &Store,
  item: &str,
  version: Option<u32>,
  mut opts: InstallOptions,
) -> Result<(Manifest, Plan), String> {
  let it = st.item(item).map_err(|e| e.to_string())?;
  if it.ext != package::EXTENSION {
    return Err(format!("'{}' is not a plugin package", it.name));
  }
  let n = version.or(it.latest().map(|v| v.number)).ok_or("the item has no versions")?;
  let bytes = st.read_version(item, Some(n)).map_err(|e| e.to_string())?;
  let pkg = Package::read(&bytes)?;
  opts.source = format!("hub:{item}/{n}");
  let plan = plugins.install(&pkg, opts)?;
  Ok((pkg.manifest, plan))
}

/// Installs the newest hub release of an installed plugin. `Ok(None)` when it is up to date.
pub fn update(plugins: &mut Plugins, st: &Store, id: &str) -> Result<Option<(Manifest, Plan)>, String> {
  let cur = plugins.get(id).ok_or_else(|| format!("no plugin '{id}' is installed"))?;
  let grant = Some(cur.record.granted.clone());
  let newest = available(plugins, st).into_iter().filter(|h| h.id == id && h.update).max_by(|a, b| {
    let v = |h: &HubPlugin| semver::Version::parse(&h.version).unwrap_or_else(|_| semver::Version::new(0, 0, 0));
    v(a).cmp(&v(b))
  });
  let Some(h) = newest else { return Ok(None) };
  // permissions the new version adds are not granted without asking
  install(plugins, st, &h.item, Some(h.hub_version), InstallOptions { grant, ..Default::default() }).map(Some)
}

/// Publishes a package to the hub: a new version of the item already holding this plugin, or a
/// new item. Returns (item, version).
pub fn publish(st: &mut Store, bytes: &[u8], project: Option<&str>, folder: Option<&str>) -> Result<(String, u32), String> {
  let pkg = Package::read(bytes)?;
  let m = &pkg.manifest;
  let file = format!("{}-{}.{}", m.id, m.version, package::EXTENSION);
  let meta = VersionMeta {
    description: format!("{} {}", m.name, m.version),
    stats: serde_json::json!({"plugin": m.id, "version": m.version}),
    ..VersionMeta::default()
  };
  let origin = Origin::Uploaded { file };
  let existing = plugin_items(st).into_iter().find(|(item, n)| {
    st.read_version(item, Some(*n)).ok().and_then(|b| package::peek_manifest(&b).ok()).is_some_and(|x| x.id == m.id)
  });
  let e = |e: fox_engine::store::StoreError| e.to_string();
  match existing {
    Some((item, _)) => Ok((item.clone(), st.add_version(&item, bytes, origin, meta).map_err(e)?)),
    None => {
      let project = match project {
        Some(p) => st.find_project(p).map_err(e)?.id.clone(),
        None => st.default_project().map_err(e)?,
      };
      let at = NewItem { project: &project, folder, name: &m.name, kind: ItemKind::File, ext: package::EXTENSION };
      st.create_item(at, bytes, origin, meta).map_err(e)
    }
  }
}
