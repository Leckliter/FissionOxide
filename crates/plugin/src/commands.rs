//! `plugin.*` JSON commands for the CLI and the MCP server (the engine knows nothing of plugins).

use std::path::{Path, PathBuf};

use fox_engine::{CommandSpec, Session};
use serde_json::{Value, json};

use crate::package::Package;
use crate::{InstallOptions, Permission, Plugins, RunOptions};

macro_rules! cmds {
  ($( $id:literal => $help:literal, $params:literal; )*) => {
    pub const COMMANDS: &[CommandSpec] = &[ $( CommandSpec { id: $id, help: $help, params: $params } ),* ];
  };
}

cmds! {
  "plugin.list" => "Installed plugins: version, enabled, permissions asked for and granted, commands, load errors", "{}";
  "plugin.commands" => "Commands of the enabled plugins with their inputs, toolbar tab and panel", "{}";
  "plugin.check" => "Check a .foxplugin file (or a plugin's folder) without installing it: manifest, code, icons; what installing would do", "{path}";
  "plugin.pack" => "Package a plugin folder (plugin.toml, plugin.wasm, icons/, resources/, README.md) into a .foxplugin", "{dir, path}";
  "plugin.install" => "Install or update a plugin from a .foxplugin file or a hub item; grants the permissions it asks for unless 'grant' lists fewer", "{path | item, version?, grant?: [permission], allow_downgrade?: bool}";
  "plugin.uninstall" => "Remove an installed plugin with its storage and log", "{id}";
  "plugin.enable" => "Turn a plugin on or off", "{id, enabled?: bool (default true)}";
  "plugin.grant" => "Set the permissions a plugin has (read-files, write-files, render, hub), out of those it asks for", "{id, permissions: [permission]}";
  "plugin.run" => "Run a plugin command on the design (one undo step); files are read from and written to 'folder'", "{command: \"<plugin id>/<command id>\", inputs?: {}, folder?}";
  "plugin.input_changed" => "What a plugin's dialog does when an input changes: updates for the other inputs", "{command, input, inputs?: {}}";
  "plugin.hub" => "Plugin packages in the data hub, with installed versions and available updates", "{}";
  "plugin.update" => "Install the newest hub release of a plugin, or of every installed plugin", "{id?}";
  "plugin.publish" => "Put a .foxplugin into the data hub (a new version of the item already holding that plugin, or a new item)", "{path, project?, folder?}";
  "plugin.log" => "A plugin's log: installs, runs, its messages and errors", "{id, clear?: bool}";
}

fn req<'a>(p: &'a Value, k: &str) -> Result<&'a str, String> {
  p.get(k).and_then(Value::as_str).ok_or_else(|| format!("missing '{k}'"))
}

fn permissions(v: &Value) -> Result<Vec<Permission>, String> {
  v.as_array()
    .ok_or("permissions must be a list")?
    .iter()
    .map(|p| {
      let s = p.as_str().unwrap_or_default();
      Permission::parse(s).ok_or_else(|| format!("unknown permission '{s}' (read-files, write-files, render, hub)"))
    })
    .collect()
}

/// Reads a package from a `.foxplugin` file or a plugin folder.
pub fn read_package(path: &Path) -> Result<Package, String> {
  if path.is_dir() {
    Package::from_dir(path)
  } else {
    Package::read(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
  }
}

pub fn package_json(pkg: &Package) -> Value {
  let m = &pkg.manifest;
  json!({
    "id": m.id,
    "name": m.name,
    "version": m.version,
    "author": m.author,
    "description": m.description,
    "permissions": m.permissions,
    "panels": m.panels,
    "commands": m.commands.iter().map(|c| json!({"id": format!("{}/{}", m.id, c.key()), "label": c.label(), "builtin": c.builtin, "step": c.step, "inputs": c.inputs.len()})).collect::<Vec<_>>(),
    "files": pkg.files.len(),
  })
}

pub fn run(plugins: &mut Plugins, s: &mut Session, name: &str, p: &Value) -> Result<Value, String> {
  let store = |s: &Session| fox_engine::hub::open_store(s).map_err(|e| e.0);
  match name {
    "plugin.list" => Ok(json!(plugins.list.iter().map(|p| p.to_json()).collect::<Vec<_>>())),
    "plugin.commands" => Ok(json!(
      plugins
        .commands()
        .iter()
        .map(|c| json!({
          "command": c.full_id,
          "label": c.command.label(),
          "builtin": c.command.builtin,
          "step": c.command.step,
          "tooltip": c.command.tooltip,
          "tab": c.tab,
          "panel": c.panel,
          "inputs": c.command.inputs,
        }))
        .collect::<Vec<_>>()
    )),
    "plugin.check" => {
      let pkg = read_package(Path::new(req(p, "path")?))?;
      let mut v = package_json(&pkg);
      v["plan"] = json!(plugins.plan(&pkg));
      Ok(v)
    }
    "plugin.pack" => {
      let pkg = Package::from_dir(Path::new(req(p, "dir")?))?;
      let out = PathBuf::from(req(p, "path")?);
      let bytes = pkg.to_bytes()?;
      std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
      let mut v = package_json(&pkg);
      v["path"] = json!(out);
      v["size"] = json!(bytes.len());
      Ok(v)
    }
    "plugin.install" => {
      let opts = InstallOptions {
        grant: p.get("grant").map(permissions).transpose()?,
        allow_downgrade: p.get("allow_downgrade").and_then(Value::as_bool).unwrap_or(false),
        source: String::new(),
      };
      let (m, plan) = match (p.get("path").and_then(Value::as_str), p.get("item").and_then(Value::as_str)) {
        (Some(path), _) => {
          let path = Path::new(path);
          if path.is_dir() {
            let pkg = Package::from_dir(path)?;
            let plan = plugins.install(&pkg, InstallOptions { source: path.to_string_lossy().into_owned(), ..opts })?;
            (pkg.manifest, plan)
          } else {
            plugins.install_file(path, opts)?
          }
        }
        (None, Some(item)) => {
          let version = p.get("version").and_then(Value::as_u64).map(|v| v as u32);
          crate::hub::install(plugins, &store(s)?, item, version, opts)?
        }
        _ => return Err("give 'path' (a .foxplugin) or 'item' (a hub item)".into()),
      };
      let granted = plugins.get(&m.id).map(|i| i.record.granted.clone()).unwrap_or_default();
      Ok(
        json!({"id": m.id, "name": m.name, "version": m.version, "plan": plan, "permissions": m.permissions, "granted": granted}),
      )
    }
    "plugin.uninstall" => {
      plugins.uninstall(req(p, "id")?)?;
      Ok(json!({"removed": req(p, "id")?}))
    }
    "plugin.enable" => {
      let on = p.get("enabled").and_then(Value::as_bool).unwrap_or(true);
      plugins.set_enabled(req(p, "id")?, on)?;
      Ok(json!({"id": req(p, "id")?, "enabled": on}))
    }
    "plugin.grant" => {
      let id = req(p, "id")?;
      plugins.set_granted(id, &permissions(p.get("permissions").ok_or("missing 'permissions'")?)?)?;
      Ok(json!({"id": id, "granted": plugins.get(id).map(|i| i.record.granted.clone())}))
    }
    "plugin.run" => {
      let opts =
        RunOptions { folder: p.get("folder").and_then(Value::as_str).map(PathBuf::from), ..Default::default() };
      let r = plugins.run(s, req(p, "command")?, p.get("inputs").unwrap_or(&Value::Null), opts)?;
      serde_json::to_value(r).map_err(|e| e.to_string())
    }
    "plugin.input_changed" => {
      plugins.input_changed(s, req(p, "command")?, req(p, "input")?, p.get("inputs").unwrap_or(&Value::Null))
    }
    "plugin.hub" => Ok(json!(crate::hub::available(plugins, &store(s)?))),
    "plugin.update" => {
      let st = store(s)?;
      let ids: Vec<String> = match p.get("id").and_then(Value::as_str) {
        Some(id) => vec![id.to_string()],
        None => plugins.list.iter().filter(|i| !i.bundled).map(|i| i.record.id.clone()).collect(),
      };
      let mut out = Vec::new();
      for id in ids {
        match crate::hub::update(plugins, &st, &id)? {
          Some((m, plan)) => out.push(json!({"id": id, "version": m.version, "plan": plan})),
          None => out.push(json!({"id": id, "up_to_date": true})),
        }
      }
      Ok(json!(out))
    }
    "plugin.publish" => {
      let path = Path::new(req(p, "path")?);
      let bytes = if path.is_dir() {
        Package::from_dir(path)?.to_bytes()?
      } else {
        std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?
      };
      let mut st = store(s)?;
      let opt = |k: &str| p.get(k).and_then(Value::as_str);
      let (item, version) = crate::hub::publish(&mut st, &bytes, opt("project"), opt("folder"))?;
      Ok(json!({"item": item, "version": version}))
    }
    "plugin.log" => {
      let id = req(p, "id")?;
      let log = plugins.log(id);
      if p.get("clear").and_then(Value::as_bool).unwrap_or(false) {
        plugins.clear_log(id);
      }
      Ok(json!({"id": id, "log": log}))
    }
    _ => Err(format!("unknown command '{name}'")),
  }
}
