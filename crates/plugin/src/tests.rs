use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use crate::manifest::Manifest;
use crate::package::Package;
use crate::sandbox::Context;
use crate::{InstallOptions, Permission, Plan, Plugins, RunOptions};

fn scratch(name: &str) -> PathBuf {
  static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let d = std::env::temp_dir().join(format!("fox-plugin-test-{}-{name}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&d);
  std::fs::create_dir_all(&d).unwrap();
  d
}

/// A session with its own scratch data hub.
fn session(hub: &Path) -> Session {
  let mut s = Session::default();
  s.store_root = Some(hub.to_path_buf());
  s
}

/// A WebAssembly text module: string constants laid out in memory, a bump allocator, and the
/// host functions imported.
#[derive(Default)]
struct Wat {
  data: String,
  next: u32,
}

impl Wat {
  /// A string constant, as the (pointer, length) operands of a call.
  fn s(&mut self, text: &str) -> String {
    let off = self.next + 16;
    let esc = text.replace('\\', "\\\\").replace('"', "\\\"");
    self.data.push_str(&format!("(data (i32.const {off}) \"{esc}\")\n"));
    self.next = off + text.len() as u32;
    format!("(i32.const {off}) (i32.const {})", text.len())
  }

  fn module(&self, run: &str) -> String {
    format!(
      r#"(module
  (import "fox" "execute" (func $execute (param i32 i32 i32 i32) (result i64)))
  (import "fox" "write_file" (func $write_file (param i32 i32 i32 i32) (result i64)))
  (import "fox" "storage_get" (func $storage_get (param i32 i32) (result i64)))
  (import "fox" "storage_set" (func $storage_set (param i32 i32 i32 i32)))
  (import "fox" "resource" (func $resource (param i32 i32) (result i64)))
  (import "fox" "log" (func $log (param i32 i32 i32)))
  (memory (export "memory") 32)
  (global $heap (mut i32) (i32.const 262144))
  (func (export "fox_alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func $str (param $p i32) (param $n i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (local.get $p)) (i64.const 32)) (i64.extend_i32_u (local.get $n))))
  {}
  (func (export "fox_run") (param i32 i32 i32 i32) (result i64) {run}))"#,
      self.data
    )
  }
}

fn manifest(id: &str, version: &str, perms: &[&str]) -> String {
  let perms: Vec<String> = perms.iter().map(|p| format!("\"{p}\"")).collect();
  format!(
    r#"id = "{id}"
name = "Test {id}"
version = "{version}"
api = 1
permissions = [{}]

[[commands]]
id = "go"
label = "Go Plugin"
"#,
    perms.join(", ")
  )
}

fn package(manifest: &str, wasm: &str) -> Package {
  let mut files = BTreeMap::new();
  files.insert("plugin.toml".to_string(), manifest.as_bytes().to_vec());
  files.insert("plugin.wasm".to_string(), wasm.as_bytes().to_vec());
  Package::from_files(files).unwrap()
}

/// Installs a one-command plugin `com.test.<name>` whose `fox_run` body is `run`.
fn install(plugins: &mut Plugins, name: &str, perms: &[&str], w: &Wat, run: &str) -> String {
  let id = format!("com.test.{name}");
  let pkg = package(&manifest(&id, "1.0.0", perms), &w.module(run));
  plugins.install(&pkg, InstallOptions { allow_downgrade: true, ..Default::default() }).unwrap();
  format!("{id}/go")
}

fn bodies(s: &Session) -> usize {
  s.model.bodies.len()
}

#[test]
fn manifest_is_checked() {
  let ok = manifest("com.example.tools", "1.2.3", &["write-files"]);
  let m = Manifest::parse(&ok).unwrap();
  assert_eq!(m.permissions, vec![Permission::WriteFiles]);
  assert_eq!(m.placement(&m.commands[0]), ("Test com.example.tools".to_string(), "TEST COM.EXAMPLE.TOOLS".to_string()));
  for (bad, why) in [
    (ok.replace("com.example.tools", "Tools"), "reverse-DNS"),
    (ok.replace("1.2.3", "one"), "version"),
    (ok.replace("api = 1", "api = 7"), "plugin API 7"),
    (ok.replace("write-files", "root"), "plugin.toml"),
    (ok.replace("api = 1", "api = 1\nfissionoxide = \">=99\""), "needs FissionOxide >=99"),
    (format!("{ok}panel = \"nope\"\n"), "no panel 'nope'"),
    (format!("{ok}[[commands]]\nid = \"go\"\nlabel = \"Again\"\n"), "repeated"),
    (
      format!("{ok}[[commands.inputs]]\nid = \"c\"\ntype = \"choice\"\noptions = [\"a\"]\ndefault = \"b\"\n"),
      "not one of the options",
    ),
    (format!("{ok}[[commands.inputs]]\nid = \"s\"\ntype = \"slider\"\n"), "min and max"),
    (format!("{ok}[[commands.inputs]]\nid = \"s\"\ntype = \"selection\"\nfilter = [\"cloud\"]\n"), "unknown filter"),
    (
      format!("{ok}[[commands.inputs]]\nid = \"t\"\ntype = \"text\"\nvisible_when = {{ other = 1 }}\n"),
      "condition on 'other'",
    ),
    (format!("{ok}unknown_key = 1\n"), "plugin.toml"),
  ] {
    let e = Manifest::parse(&bad).expect_err(&bad);
    assert!(e.contains(why), "{e} (wanted {why})");
  }
}

#[test]
fn modules_are_checked() {
  let m = manifest("com.test.check", "1.0.0", &[]);
  let mut files = BTreeMap::new();
  files.insert("plugin.toml".to_string(), m.as_bytes().to_vec());
  let check = |files: &BTreeMap<String, Vec<u8>>, wasm: &str| {
    let mut f = files.clone();
    f.insert("plugin.wasm".into(), wasm.as_bytes().to_vec());
    Package::from_files(f).map(|_| ()).unwrap_err()
  };
  assert!(check(&files, "(module)").contains("memory"), "no memory");
  assert!(check(&files, "(module (memory (export \"memory\") 1))").contains("fox_alloc"));
  let wasi = r#"(module (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32))))"#;
  assert!(check(&files, wasi).contains("wasi_snapshot_preview1.fd_write"));
  let bad_sig = r#"(module (import "fox" "execute" (func (param i32) (result i64))))"#;
  assert!(check(&files, bad_sig).contains("wrong signature"));
  assert!(check(&files, "not wasm").contains("doesn't compile"));
  // a command icon must be in the package
  let with_icon = m.replace("label = \"Go Plugin\"", "label = \"Go Plugin\"\nicon = \"icons/go.png\"");
  let mut f = BTreeMap::new();
  f.insert("plugin.toml".to_string(), with_icon.as_bytes().to_vec());
  f.insert("plugin.wasm".to_string(), Wat::default().module("(i64.const 0)").into_bytes());
  assert!(Package::from_files(f.clone()).unwrap_err().contains("no icon"));
  f.insert("icons/go.png".to_string(), vec![0x89, b'P', b'N', b'G']);
  let pkg = Package::from_files(f).unwrap();
  // and survives a round trip through a .foxplugin
  let again = Package::read(&pkg.to_bytes().unwrap()).unwrap();
  assert_eq!(again.files, pkg.files);
  assert!(Package::read(b"not a zip").unwrap_err().contains("not a .foxplugin"));
}

#[test]
fn a_run_is_one_undo_step_and_failures_roll_back() {
  let dir = scratch("run");
  let mut s = session(&dir);
  let mut p = Plugins::open(&dir.join("plugins"), vec![]);
  let mut w = Wat::default();
  let (b, params) = (w.s("box"), w.s(r#"{"width": 10, "depth": 10, "height": 10}"#));
  let body = format!(
    "(drop (call $execute {b} {params})) (drop (call $execute {b} {params})) (call $str {})",
    w.s(r#"{"ok": {"made": 2}}"#)
  );
  let id = install(&mut p, "good", &[], &w, &body);
  let r = p.run(&mut s, &id, &json!({}), RunOptions::default()).unwrap();
  assert_eq!(r.result, json!({"made": 2}));
  assert!(r.changed);
  assert_eq!(r.commands, 2);
  assert_eq!(bodies(&s), 2);
  assert_eq!(s.undo_history(), vec!["Go Plugin".to_string()], "two boxes, one undo step");
  assert!(s.undo());
  assert_eq!(bodies(&s), 0);

  // an error reply undoes what the plugin did
  let mut w = Wat::default();
  let (b, params) = (w.s("box"), w.s(r#"{"width": 10, "depth": 10, "height": 10}"#));
  let body = format!("(drop (call $execute {b} {params})) (call $str {})", w.s(r#"{"error": "it went wrong"}"#));
  let id = install(&mut p, "bad", &[], &w, &body);
  assert_eq!(p.run(&mut s, &id, &json!({}), RunOptions::default()).unwrap_err(), "it went wrong");
  assert_eq!(bodies(&s), 0);
  assert!(!s.in_transaction());

  // so does a trap, and running forever
  let mut w = Wat::default();
  let body =
    format!("(drop (call $execute {} {})) (unreachable)", w.s("box"), w.s(r#"{"width": 1, "depth": 1, "height": 1}"#));
  let id = install(&mut p, "trap", &[], &w, &body);
  assert!(p.run(&mut s, &id, &json!({}), RunOptions::default()).unwrap_err().contains("crashed"));
  assert_eq!(bodies(&s), 0);
  let id = install(&mut p, "spin", &[], &Wat::default(), "(loop $l (br $l)) (i64.const 0)");
  let opts = RunOptions { fuel: Some(1_000_000), ..Default::default() };
  assert!(p.run(&mut s, &id, &json!({}), opts).unwrap_err().contains("ran too long"));
  // a reply that isn't JSON
  let mut w = Wat::default();
  let body = format!("(call $str {})", w.s("hello"));
  let id = install(&mut p, "garbage", &[], &w, &body);
  assert!(p.run(&mut s, &id, &json!({}), RunOptions::default()).unwrap_err().contains("not JSON"));
  assert!(!s.can_undo());
  // the log has each run
  let log = p.log("com.test.bad");
  assert!(log.contains("installed version 1.0.0") && log.contains("failed: it went wrong"), "{log}");
}

#[test]
fn commands_are_sandboxed() {
  let dir = scratch("sandbox");
  let mut s = session(&dir);
  let mut p = Plugins::open(&dir.join("plugins"), vec![]);
  // the plugin passes the host's reply straight back, so its error is the run's error
  let forward =
    |p: &mut Plugins, s: &mut Session, name: &str, perms: &[&str], cmd: &str, params: Value, folder: Option<&Path>| {
      let mut w = Wat::default();
      let body = format!("(call $execute {} {})", w.s(cmd), w.s(&params.to_string()));
      let id = install(p, name, perms, &w, &body);
      let opts = RunOptions { folder: folder.map(Path::to_path_buf), ..Default::default() };
      p.run(s, &id, &json!({}), opts)
    };
  for cmd in ["open", "new", "save", "undo", "transaction.commit", "hub.open"] {
    let e = forward(&mut p, &mut s, "refused", &[], cmd, json!({"path": "x.fox"}), None).unwrap_err();
    assert!(e.contains("plugins can't run"), "{cmd}: {e}");
  }
  let out = scratch("sandbox-out");
  let e = forward(&mut p, &mut s, "noperm", &[], "export", json!({"path": "a.step"}), Some(&out)).unwrap_err();
  assert!(e.contains("'write-files' permission"), "{e}");
  let e = forward(&mut p, &mut s, "hub", &[], "hub.projects", json!({}), None).unwrap_err();
  assert!(e.contains("'hub' permission"), "{e}");
  let e = forward(&mut p, &mut s, "render", &["write-files"], "view.render", json!({"path": "a.png"}), Some(&out))
    .unwrap_err();
  assert!(e.contains("'render' permission"), "{e}");
  let e = forward(&mut p, &mut s, "nofolder", &["write-files"], "export", json!({"path": "a.step"}), None).unwrap_err();
  assert!(e.contains("pick a folder"), "{e}");
  let e = forward(&mut p, &mut s, "escape", &["write-files"], "export", json!({"path": "../a.step"}), Some(&out))
    .unwrap_err();
  assert!(e.contains("without '..'"), "{e}");
  let e =
    forward(&mut p, &mut s, "abs", &["write-files"], "export", json!({"path": "/tmp/a.step"}), Some(&out)).unwrap_err();
  assert!(e.contains("relative"), "{e}");
  // granted: the path lands in the run's folder
  s.execute("box", &json!({"width": 5, "depth": 5, "height": 5})).unwrap();
  forward(&mut p, &mut s, "export", &["write-files"], "export", json!({"path": "sub/a.stl"}), Some(&out)).unwrap();
  assert!(out.join("sub/a.stl").is_file());
  // the user can take a permission back
  p.set_granted("com.test.export", &[]).unwrap();
  let id = "com.test.export/go";
  let e = p.run(&mut s, id, &json!({}), RunOptions { folder: Some(out.clone()), ..Default::default() }).unwrap_err();
  assert!(e.contains("'write-files' permission"), "{e}");

  // path rules on their own
  let ctx = Context { granted: vec![Permission::ReadFiles], folder: Some(out.clone()), resources: dir.join("res") };
  let r = ctx.prepare("import", &json!({"path": "in.step"})).unwrap();
  assert_eq!(r["path"], json!(out.join("in.step")));
  let r = ctx.prepare("sketch.insert", &json!({"from": {"path": "resources/logo.dxf"}})).unwrap();
  assert_eq!(r["from"]["path"], json!(dir.join("res").join("logo.dxf")));
  let r = ctx.prepare("sketch.text", &json!({"font": "mono", "text": "A"})).unwrap();
  assert_eq!(r["font"], "mono", "font names are not paths");
  assert!(ctx.prepare("sketch.text", &json!({"font": "../x.ttf"})).is_err());
  // a command the table doesn't know is still confined
  assert!(ctx.prepare("someday.write", &json!({"options": {"file": "out.txt"}})).unwrap_err().contains("write-files"));
  let r = ctx.prepare("sweep", &json!({"path": 3})).unwrap();
  assert_eq!(r["path"], 3, "ids are left alone");
  #[cfg(unix)]
  {
    std::os::unix::fs::symlink(std::env::temp_dir(), out.join("link")).unwrap();
    let ctx = Context { granted: vec![Permission::WriteFiles], ..ctx };
    assert!(ctx.prepare("export", &json!({"path": "link/a.step"})).unwrap_err().contains("outside"));
  }
}

#[test]
fn files_storage_and_resources() {
  let dir = scratch("files");
  let mut s = session(&dir);
  let mut p = Plugins::open(&dir.join("plugins"), vec![]);
  let out = scratch("files-out");
  let mut w = Wat::default();
  let fail = w.s(r#"{"error": "write failed"}"#);
  let body = format!(
    "(if (i64.ne (call $write_file {} {}) (i64.const 0)) (then (return (call $str {fail})))) \
     (call $storage_set {} {}) (call $log (i32.const 2) {}) (call $str {})",
    w.s("notes/hello.txt"),
    w.s("hi there"),
    w.s("answer"),
    w.s(r#"{"ok": "from storage"}"#),
    w.s("wrote it"),
    w.s(r#"{"ok": null}"#)
  );
  let id = install(&mut p, "writer", &["write-files"], &w, &body);
  let r = p.run(&mut s, &id, &json!({}), RunOptions { folder: Some(out.clone()), ..Default::default() }).unwrap();
  assert_eq!(std::fs::read_to_string(out.join("notes/hello.txt")).unwrap(), "hi there");
  assert_eq!(r.files, vec![out.join("notes/hello.txt")]);
  assert_eq!(r.log[0].message, "wrote it");
  assert!(!r.changed);
  assert!(p.log("com.test.writer").contains("[info] wrote it"));
  p.set_granted("com.test.writer", &[]).unwrap();
  let e = p.run(&mut s, &id, &json!({}), RunOptions { folder: Some(out.clone()), ..Default::default() }).unwrap_err();
  assert_eq!(e, "write failed");

  // storage survives between runs (and the plugin's reply can come straight from it)
  let mut w = Wat::default();
  let body = format!("(call $storage_get {})", w.s("answer"));
  let pkg = package(&manifest("com.test.writer", "1.1.0", &["write-files"]), &w.module(&body));
  assert_eq!(p.install(&pkg, InstallOptions::default()).unwrap(), Plan::Update { from: "1.0.0".into() });
  let r = p.run(&mut s, &id, &json!({}), RunOptions::default()).unwrap();
  assert_eq!(r.result, json!("from storage"), "storage survives an update");

  // resources: bundled files the plugin reads back
  let mut w = Wat::default();
  let body = format!("(call $resource {})", w.s("resources/reply.json"));
  let mut files = BTreeMap::new();
  files.insert("plugin.toml".to_string(), manifest("com.test.res", "1.0.0", &[]).into_bytes());
  files.insert("plugin.wasm".to_string(), w.module(&body).into_bytes());
  files.insert("resources/reply.json".to_string(), br#"{"ok": [1, 2]}"#.to_vec());
  p.install(&Package::from_files(files).unwrap(), InstallOptions::default()).unwrap();
  let r = p.run(&mut s, "com.test.res/go", &json!({}), RunOptions::default()).unwrap();
  assert_eq!(r.result, json!([1, 2]));
}

#[test]
fn install_update_and_remove() {
  let dir = scratch("install");
  let root = dir.join("plugins");
  let mut p = Plugins::open(&root, vec![]);
  let w = Wat::default().module(r#"(call $str (i32.const 0) (i32.const 0))"#);
  let v1 = package(&manifest("com.test.life", "1.0.0", &["write-files", "render"]), &w);
  let v2 = package(&manifest("com.test.life", "2.0.0", &["write-files", "render"]), &w);
  assert_eq!(p.plan(&v1), Plan::Install);
  let grant = InstallOptions { grant: Some(vec![Permission::Render]), ..Default::default() };
  assert_eq!(p.install(&v1, grant).unwrap(), Plan::Install);
  assert!(root.join("com.test.life/1.0.0/plugin.wasm").is_file());
  assert_eq!(p.get("com.test.life").unwrap().record.granted, vec![Permission::Render]);
  assert_eq!(p.commands().len(), 1);
  assert!(p.install(&v1, InstallOptions::default()).unwrap_err().contains("already installed"));
  p.set_enabled("com.test.life", false).unwrap();
  assert!(p.commands().is_empty());
  assert!(p.command("com.test.life/go").unwrap_err().contains("turned off"));
  // an update keeps the enabled flag and replaces the old version
  assert_eq!(p.install(&v2, InstallOptions::default()).unwrap(), Plan::Update { from: "1.0.0".into() });
  assert!(!root.join("com.test.life/1.0.0").exists() && root.join("com.test.life/2.0.0").is_dir());
  assert!(!p.get("com.test.life").unwrap().record.enabled);
  assert_eq!(p.plan(&v1), Plan::Downgrade { from: "2.0.0".into() });
  assert!(p.install(&v1, InstallOptions::default()).unwrap_err().contains("older"));
  let back = InstallOptions { allow_downgrade: true, ..Default::default() };
  assert_eq!(p.install(&v1, back).unwrap(), Plan::Downgrade { from: "2.0.0".into() });
  // the registry is read back from disk
  let again = Plugins::open(&root, vec![]);
  assert_eq!(again.get("com.test.life").unwrap().record.version, "1.0.0");
  // a broken install is reported and skipped, without stopping the others
  std::fs::write(root.join("com.test.life/1.0.0/plugin.toml"), "id = 3").unwrap();
  let broken = Plugins::open(&root, vec![]);
  assert!(broken.get("com.test.life").unwrap().error.is_some());
  assert!(broken.commands().is_empty());
  p.uninstall("com.test.life").unwrap();
  assert!(p.get("com.test.life").is_none() && !root.join("com.test.life").exists());
  assert!(p.uninstall("com.test.life").is_err());

  // bundled plugins: listed, can be turned off but not removed; an installed copy wins
  let bundled = dir.join("bundled");
  v1.unpack(&bundled.join("life")).unwrap();
  let mut p = Plugins::open(&root, vec![bundled.clone()]);
  let b = p.get("com.test.life").unwrap();
  assert!(b.bundled && b.usable());
  assert!(p.uninstall("com.test.life").unwrap_err().contains("turned off"));
  p.set_enabled("com.test.life", false).unwrap();
  assert!(!p.get("com.test.life").unwrap().usable());
  p.install(&v2, InstallOptions::default()).unwrap();
  let b = p.get("com.test.life").unwrap();
  assert!(!b.bundled && b.record.version == "2.0.0");
}

#[test]
fn inputs_are_checked_and_filled_in() {
  let text = format!(
    "{}{}",
    manifest("com.test.inputs", "1.0.0", &[]),
    r#"
  [[commands.inputs]]
  id = "body"
  type = "selection"
  filter = ["body"]
  max = 1

  [[commands.inputs]]
  id = "size"
  type = "value"
  default = "1 in"
  min = 1

  [[commands.inputs]]
  id = "turn"
  type = "value"
  unit = "angle"
  default = 45

  [[commands.inputs]]
  id = "count"
  type = "integer"
  default = 3
  min = 1
  max = 10

  [[commands.inputs]]
  id = "mode"
  type = "choice"
  options = ["a", "b"]

  [[commands.inputs]]
  id = "note"
  type = "label"
  default = "hello"
"#
  );
  let m = Manifest::parse(&text).unwrap();
  let cmd = &m.commands[0];
  let mut s = Session::default();
  s.execute("box", &json!({"width": 5, "depth": 5, "height": 5})).unwrap();
  let resolve = |s: &Session, v: Value| crate::inputs::resolve(cmd, &v, s);
  assert!(resolve(&s, json!({})).unwrap_err().contains("select at least 1"));
  let body = s.model.bodies[0].id;
  s.selection = vec![Sel::Body { body }];
  let v = resolve(&s, json!({})).unwrap();
  assert_eq!(v["body"], json!([{"type": "body", "body": body.0}]), "taken from the selection");
  assert!((v["size"].as_f64().unwrap() - 25.4).abs() < 1e-9);
  assert!((v["turn"].as_f64().unwrap() - 45.0).abs() < 1e-9, "angles in degrees");
  assert_eq!(v["count"], 3);
  assert_eq!(v["mode"], "a");
  assert!(v.get("note").is_none(), "labels carry no value");
  let v = resolve(&s, json!({"size": "2 * 3 mm", "turn": "0.5 rad", "count": 4, "mode": "b"})).unwrap();
  assert!((v["size"].as_f64().unwrap() - 6.0).abs() < 1e-9);
  assert!((v["turn"].as_f64().unwrap() - 0.5f64.to_degrees()).abs() < 1e-9);
  for (bad, why) in [
    (json!({"count": 11}), "at most 10"),
    (json!({"count": 2.5}), "whole number"),
    (json!({"size": 0.5}), "at least 1"),
    (json!({"size": "1 parsec"}), "size"),
    (json!({"mode": "c"}), "one of a, b"),
    (json!({"nope": 1}), "no input 'nope'"),
    (json!({"note": "x"}), "no input 'note'"),
    (json!({"body": [{"type": "face", "body": body.0, "face": 0}]}), "can't pick a face"),
  ] {
    let e = resolve(&s, bad.clone()).unwrap_err();
    assert!(e.contains(why), "{bad}: {e}");
  }
  s.selection = vec![Sel::Face { body, face: 0 }];
  assert!(resolve(&s, json!({})).unwrap_err().contains("select at least 1"), "faces don't pass a body filter");
}

fn example_dir() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/flange")
}

#[test]
fn the_example_plugin_works() {
  let dir = scratch("example");
  let mut s = session(&dir);
  let mut p = Plugins::open(&dir.join("plugins"), vec![]);
  let pkg = Package::from_dir(&example_dir()).unwrap();
  assert_eq!(pkg.manifest.id, "org.fissionoxide.examples.flange");
  assert!(pkg.files.contains_key("icons/flange.png") && pkg.files.contains_key("resources/report-header.txt"));
  // packed and read back, as installing a .foxplugin does
  let pkg = Package::read(&pkg.to_bytes().unwrap()).unwrap();
  p.install(&pkg, InstallOptions::default()).unwrap();
  let cmds = p.commands();
  assert_eq!(
    cmds.iter().map(|c| c.full_id.as_str()).collect::<Vec<_>>(),
    ["org.fissionoxide.examples.flange/flange", "org.fissionoxide.examples.flange/report"]
  );
  assert_eq!((cmds[0].tab.as_str(), cmds[0].panel.as_str()), ("Flange Tools", "FLANGES"));
  assert!(cmds[0].icon.as_ref().unwrap().is_file());

  // the dialog's hint follows the number of holes
  let flange = "org.fissionoxide.examples.flange/flange";
  let u = p.input_changed(&mut s, flange, "holes", &json!({"holes": 8, "outer": 100, "bore": 20})).unwrap();
  assert_eq!(u["spacing"]["text"], "A hole every 45°");
  assert_eq!(u["pitch"]["value"], "60 mm");
  assert_eq!(bodies(&s), 0);

  let r = p
    .run(
      &mut s,
      flange,
      &json!({"outer": "100 mm", "bore": 30, "thickness": 10, "holes": 6, "hole": 8}),
      RunOptions::default(),
    )
    .unwrap();
  assert_eq!(r.result["flanges_made"], 1);
  assert_eq!(bodies(&s), 1);
  let info = s.execute("info", &json!({})).unwrap();
  let b = &info["bodies"][0];
  assert_eq!(b["name"], "Flange 1");
  let want = std::f64::consts::PI / 4.0 * (100.0f64.powi(2) - 30.0f64.powi(2) - 6.0 * 64.0) * 10.0;
  let vol = b["volume_mm3"].as_f64().unwrap();
  assert!((vol - want).abs() / want < 1e-3, "{vol} vs {want}");
  assert_eq!(s.undo_history()[0], "Bolt Flange");
  // the plugin's own checks reject a bad flange, leaving the design as it was
  let e = p.run(&mut s, flange, &json!({"holes": 40}), RunOptions::default()).unwrap_err();
  assert!(e.contains("overlap"), "{e}");
  assert_eq!(bodies(&s), 1);
  // a second flange: storage counts them
  let r = p.run(&mut s, flange, &json!({"plane": "XZ", "holes": 4}), RunOptions::default()).unwrap();
  assert_eq!(r.result["flanges_made"], 2);

  let out = scratch("example-out");
  let report = "org.fissionoxide.examples.flange/report";
  let r = p.run(&mut s, report, &json!({}), RunOptions { folder: Some(out.clone()), ..Default::default() }).unwrap();
  assert_eq!(r.result, json!({"file": "bodies.csv", "bodies": 2}));
  let csv = std::fs::read_to_string(out.join("bodies.csv")).unwrap();
  let lines: Vec<&str> = csv.lines().collect();
  assert!(lines[0].starts_with("# Body report"), "{csv}");
  assert!(lines[1].starts_with("id,name,volume_mm3"));
  assert!(lines[2].contains(",Flange 1,") && lines[2].ends_with(",100,100,10"), "{csv}");
  assert_eq!(lines.len(), 4);
}

#[test]
fn plugins_through_the_data_hub() {
  let dir = scratch("hub");
  let mut s = session(&dir.join("hub"));
  let mut p = Plugins::open(&dir.join("plugins"), vec![]);
  let w = Wat::default().module(r#"(call $str (i32.const 0) (i32.const 0))"#);
  let bytes = |v: &str| package(&manifest("com.test.shared", v, &["hub"]), &w).to_bytes().unwrap();
  let run = |p: &mut Plugins, s: &mut Session, cmd: &str, params: Value| crate::commands::run(p, s, cmd, &params);
  let f1 = dir.join("shared-1.foxplugin");
  std::fs::write(&f1, bytes("1.0.0")).unwrap();
  let r = run(&mut p, &mut s, "plugin.publish", json!({"path": f1})).unwrap();
  let item = r["item"].as_str().unwrap().to_string();
  assert_eq!(r["version"], 1);
  let hub = run(&mut p, &mut s, "plugin.hub", json!({})).unwrap();
  assert_eq!(hub[0]["id"], "com.test.shared");
  assert_eq!(hub[0]["installed"], Value::Null);
  let r = run(&mut p, &mut s, "plugin.install", json!({"item": item, "grant": []})).unwrap();
  assert_eq!(r["plan"]["action"], "install");
  assert_eq!(r["granted"], json!([]));
  assert_eq!(p.get("com.test.shared").unwrap().record.source, format!("hub:{item}/1"));
  // nothing newer yet
  assert_eq!(run(&mut p, &mut s, "plugin.update", json!({})).unwrap()[0]["up_to_date"], true);
  // a new release goes in as a new version of the same item, and updates installs
  let f2 = dir.join("shared-2.foxplugin");
  std::fs::write(&f2, bytes("1.1.0")).unwrap();
  let r = run(&mut p, &mut s, "plugin.publish", json!({"path": f2})).unwrap();
  assert_eq!((r["item"].as_str().unwrap(), r["version"].as_u64().unwrap()), (item.as_str(), 2));
  let hub = run(&mut p, &mut s, "plugin.hub", json!({})).unwrap();
  assert_eq!((hub[0]["installed"].as_str(), hub[0]["update"].as_bool()), (Some("1.0.0"), Some(true)));
  let r = run(&mut p, &mut s, "plugin.update", json!({"id": "com.test.shared"})).unwrap();
  assert_eq!(r[0]["version"], "1.1.0");
  let installed = p.get("com.test.shared").unwrap();
  assert_eq!(installed.record.version, "1.1.0");
  assert!(installed.record.granted.is_empty(), "an update doesn't grant what the user turned down");

  // the rest of the command set
  let list = run(&mut p, &mut s, "plugin.list", json!({})).unwrap();
  assert_eq!(list[0]["version"], "1.1.0");
  let check = run(&mut p, &mut s, "plugin.check", json!({"path": f1})).unwrap();
  assert_eq!(check["plan"]["action"], "downgrade");
  let packed = dir.join("example.foxplugin");
  run(&mut p, &mut s, "plugin.pack", json!({"dir": example_dir(), "path": packed})).unwrap();
  let r = run(&mut p, &mut s, "plugin.install", json!({"path": packed})).unwrap();
  assert_eq!(r["granted"], json!(["write-files"]));
  let cmds = run(&mut p, &mut s, "plugin.commands", json!({})).unwrap();
  assert_eq!(cmds.as_array().unwrap().len(), 3);
  run(&mut p, &mut s, "plugin.enable", json!({"id": "com.test.shared", "enabled": false})).unwrap();
  assert_eq!(run(&mut p, &mut s, "plugin.commands", json!({})).unwrap().as_array().unwrap().len(), 2);
  let r =
    run(&mut p, &mut s, "plugin.grant", json!({"id": "com.test.shared", "permissions": ["hub", "render"]})).unwrap();
  assert_eq!(r["granted"], json!(["hub"]), "only what it asks for");
  assert!(run(&mut p, &mut s, "plugin.grant", json!({"id": "com.test.shared", "permissions": ["root"]})).is_err());
  let r = run(
    &mut p,
    &mut s,
    "plugin.run",
    json!({"command": "org.fissionoxide.examples.flange/flange", "inputs": {"holes": 3}}),
  )
  .unwrap();
  assert_eq!(r["changed"], true);
  assert!(
    run(&mut p, &mut s, "plugin.log", json!({"id": "org.fissionoxide.examples.flange"})).unwrap()["log"]
      .as_str()
      .unwrap()
      .contains("Bolt Flange: done")
  );
  run(&mut p, &mut s, "plugin.uninstall", json!({"id": "com.test.shared"})).unwrap();
  assert_eq!(run(&mut p, &mut s, "plugin.list", json!({})).unwrap().as_array().unwrap().len(), 1);
}

#[test]
fn a_tab_can_hold_builtin_commands_in_step_order() {
  let dir = scratch("builtin");
  let mut s = session(&dir);
  let mut p = Plugins::open(&dir.join("plugins"), vec![]);
  let text = format!(
    "{}{}",
    manifest("com.test.flow", "1.0.0", &[]),
    r#"step = 3

[[commands]]
builtin = "box"
label = "Make the Blank"
step = 1

  [[commands.inputs]]
  id = "width"
  type = "value"
  default = "20 mm"

  [[commands.inputs]]
  id = "depth"
  type = "value"
  default = 10

  [[commands.inputs]]
  id = "height"
  type = "value"
  default = "0.5 in"

[[commands]]
builtin = "body.rename"
step = 2

  [[commands.inputs]]
  id = "body"
  type = "selection"
  filter = ["body"]
  max = 1

  [[commands.inputs]]
  id = "name"
  type = "text"
  default = "Blank"
"#
  );
  let m = Manifest::parse(&text).unwrap();
  assert_eq!(m.command("box").unwrap().label(), "Make the Blank");
  assert_eq!(m.command("body.rename").unwrap().label(), "body.rename");
  let pkg = package(&text, &Wat::default().module("(call $str (i32.const 0) (i32.const 0))"));
  p.install(&pkg, InstallOptions::default()).unwrap();
  let ids: Vec<String> = p.commands().into_iter().map(|c| c.full_id).collect();
  assert_eq!(ids, ["com.test.flow/box", "com.test.flow/body.rename", "com.test.flow/go"], "in step order");
  let r = p.run(&mut s, "com.test.flow/box", &json!({}), RunOptions::default()).unwrap();
  assert!(r.changed);
  let bb = s.model.bodies[0].id;
  let info = s.execute("info", &json!({})).unwrap();
  let size = &info["bodies"][0]["bbox"];
  let dz = size[1][2].as_f64().unwrap() - size[0][2].as_f64().unwrap();
  assert!((dz - 12.7).abs() < 1e-6, "{size}");
  assert_eq!(s.undo_history()[0], "Make the Blank");
  s.selection = vec![Sel::Body { body: bb }];
  p.run(&mut s, "com.test.flow/body.rename", &json!({}), RunOptions::default()).unwrap();
  assert_eq!(s.body_name(bb), "Blank");
  // the same rules as a plugin's own calls
  for bad in [
    format!("{}[[commands]]\nbuiltin = \"open\"\n", manifest("com.test.open", "1.0.0", &[])),
    format!("{}[[commands]]\nbuiltin = \"hub.item.purge\"\n", manifest("com.test.purge", "1.0.0", &[])),
  ] {
    let pkg = package(&bad, &Wat::default().module("(i64.const 0)"));
    p.install(&pkg, InstallOptions::default()).unwrap();
    let id = format!("{}/{}", pkg.manifest.id, pkg.manifest.commands[1].key());
    assert!(p.run(&mut s, &id, &json!({}), RunOptions::default()).is_err(), "{id}");
  }
  for bad in [
    "[[commands]]\nbuiltin = \"box\"\nid = \"x\"\n",
    "[[commands]]\nbuiltin = \"Box!\"\n",
    "[[commands]]\nid = \"nolabel\"\n",
  ] {
    assert!(Manifest::parse(&format!("{}{bad}", manifest("com.test.bad", "1.0.0", &[]))).is_err(), "{bad}");
  }
}

#[test]
fn bundled_add_ins_can_start_off_and_ship_a_library() {
  let dir = scratch("addin");
  let bundled = dir.join("addins");
  let addin = bundled.join("lumen");
  std::fs::create_dir_all(addin.join("library/Pack")).unwrap();
  let text = "id = \"com.test.addin\"\nname = \"Add-in\"\nversion = \"1.0.0\"\napi = 1\nlibrary = \"library\"\n\
              enabled_by_default = false\n\n[[commands]]\nid = \"go\"\nlabel = \"Go\"\n";
  std::fs::write(addin.join("plugin.toml"), text).unwrap();
  std::fs::write(addin.join("plugin.wasm"), Wat::default().module("(i64.const 0)")).unwrap();
  std::fs::write(addin.join("library/Pack/disc.dxf"), "0\nEOF\n").unwrap();
  let lib = addin.join("library");
  let mut p = Plugins::open(&dir.join("plugins"), vec![bundled.clone()]);
  let a = p.get("com.test.addin").unwrap();
  assert!(a.bundled && !a.record.enabled, "starts off");
  assert!(p.commands().is_empty());
  assert!(!p.libraries.contains(&lib));
  p.set_enabled("com.test.addin", true).unwrap();
  assert!(p.get("com.test.addin").unwrap().record.enabled);
  assert_eq!(p.libraries, vec![lib.clone()], "its library is offered while it is on");
  p.reload();
  assert!(p.get("com.test.addin").unwrap().record.enabled, "remembered");
  p.set_enabled("com.test.addin", false).unwrap();
  assert!(p.libraries.is_empty());
  // Run starts it for this session only; Run on Startup is remembered and leaves it as it runs
  p.set_running("com.test.addin", true).unwrap();
  let a = p.get("com.test.addin").unwrap();
  assert!(a.record.enabled && !a.startup && !p.commands().is_empty());
  p.set_run_on_startup("com.test.addin", true).unwrap();
  p.set_running("com.test.addin", false).unwrap();
  let a = p.get("com.test.addin").unwrap();
  assert!(!a.record.enabled && a.startup && p.commands().is_empty());
  let fresh = Plugins::open(&dir.join("plugins"), vec![bundled.clone()]);
  let a = fresh.get("com.test.addin").unwrap();
  assert!(a.record.enabled && a.startup, "it starts with the app next time");
  assert!(Manifest::parse(&text.replace("library = \"library\"", "library = \"../up\"")).is_err());
}

#[test]
fn new_file_commands_are_confined() {
  let dir = scratch("confine");
  let out = scratch("confine-out");
  let res = dir.join("res");
  let all = vec![Permission::ReadFiles, Permission::WriteFiles, Permission::Render];
  let ctx = Context { granted: all, folder: Some(out.clone()), resources: res.clone() };
  let r = ctx.prepare("view.render_components", &json!({"folder": "Parts"})).unwrap();
  assert_eq!(r["folder"], json!(out.join("Parts")), "a bare folder name is still inside the run's folder");
  let r = ctx.prepare("cut.estimate", &json!({"sketch": 3, "export": "est.csv"})).unwrap();
  assert_eq!(r["export"], json!(out.join("est.csv")));
  assert!(ctx.prepare("cut.service.check", &json!({"dxf": "/etc/passwd"})).is_err());
  assert!(ctx.prepare("inspect.overall_size", &json!({"export": "../x.json"})).is_err());
  let r = ctx.prepare("sketch.library.insert", &json!({"part": "LumenArt/keyhole", "at": [0, 0]})).unwrap();
  assert_eq!(r["part"], "LumenArt/keyhole", "pack/name is not a path");
  let r = ctx.prepare("sketch.library.insert", &json!({"part": "resources/a.dxf", "at": [0, 0]})).unwrap();
  assert_eq!(r["part"], json!(res.join("a.dxf")));
  assert!(ctx.prepare("sketch.library.list", &json!({"folders": ["C:/"]})).is_err());
  let render_only = Context { granted: vec![Permission::Render], folder: Some(out.clone()), resources: res.clone() };
  assert!(render_only.prepare("view.render_components", &json!({"folder": "Parts"})).is_ok());
  let nothing = Context { granted: vec![], folder: Some(out), resources: res };
  assert!(nothing.prepare("view.render_components", &json!({"folder": "Parts"})).unwrap_err().contains("render"));
  assert!(nothing.prepare("sheet.collect_flat", &json!({"path": "cut.dxf"})).unwrap_err().contains("write-files"));
}
/// The LumenArt add-in in the repository's `addins/` builds a two-layer sign: numbered layers,
/// tab stubs half a thou proud, and flanges over the standoff gap plus the bend allowance.
#[test]
fn lumenart_add_in_builds_a_layered_sign() {
  let addins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../addins");
  if !addins.join("lumenart/plugin.wasm").is_file() {
    return; // not built in this checkout
  }
  let dir = scratch("lumenart");
  let mut s = session(&dir);
  let mut p = Plugins::open(&dir.join("plugins"), vec![addins]);
  let id = "shop.lumenart.signs";
  assert!(!p.get(id).unwrap().record.enabled, "an add-in for one trade starts off");
  p.set_enabled(id, true).unwrap();
  let run = |p: &mut Plugins, s: &mut Session, cmd: &str, inputs: Value| {
    p.run(s, &format!("{id}/{cmd}"), &inputs, RunOptions::default()).unwrap_or_else(|e| panic!("{cmd}: {e}")).result
  };
  for (lo, hi) in [([0, 0], [200, 100]), ([50, 25], [150, 75])] {
    s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    s.execute("sketch.rect", &json!({"corner1": lo, "corner2": hi})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
  }
  let sketches: Vec<u32> = s.design.features.iter().map(|f| f.id.0).collect();
  run(
    &mut p,
    &mut s,
    "new_layer",
    json!({"profiles": [{"type": "profile", "sketch": sketches[0], "index": 0}], "name": "Back"}),
  );
  let r = run(
    &mut p,
    &mut s,
    "new_layer",
    json!({"profiles": [{"type": "profile", "sketch": sketches[1], "index": 0}], "name": "Letters", "height": "1 in"}),
  );
  assert_eq!(r["component"], "02 - Letters");
  let letters = r["body"].as_u64().unwrap();
  let topo = s.execute("body.topology", &json!({"body": letters})).unwrap();
  let edge_at =
    |x: f64| {
      topo["edges"].as_array().unwrap().iter().find(|e| {
        (e["mid"][0].as_f64().unwrap() - x).abs() < 0.01 && (e["mid"][2].as_f64().unwrap() - 27.0).abs() < 0.01
      })
    };
  let edges: Vec<Value> = [150.0, 50.0]
    .iter()
    .map(|x| json!({"type": "edge", "body": letters, "edge": edge_at(*x).unwrap()["id"]}))
    .collect();
  run(&mut p, &mut s, "quick_tab", json!({"edges": edges}));
  let span = |s: &mut Session| {
    let info = s.execute("info", &json!({})).unwrap();
    let b = info["bodies"].as_array().unwrap().iter().find(|b| b["id"] == letters).unwrap().clone();
    (b["bbox"][0][0].as_f64().unwrap(), b["bbox"][1][0].as_f64().unwrap())
  };
  let (lo, hi) = span(&mut s);
  assert!((lo - (50.0 - 0.127)).abs() < 1e-3 && (hi - (150.0 + 0.127)).abs() < 1e-3, "tabs 0.005\" proud: {lo} {hi}");
  let r = run(&mut p, &mut s, "auto_flange", json!({}));
  assert_eq!(r["flanges"], 2, "{r}");
  // gap from the tab's middle (25.4 + 0.8) down to the back's top (1.6), plus 0.025"
  let len = 26.2001 - 1.6002 + 0.635;
  let (lo2, hi2) = span(&mut s);
  assert!((lo - lo2 - len).abs() < 1e-3 && (hi2 - hi - len).abs() < 1e-3, "{lo2} {hi2}");
  assert_eq!(run(&mut p, &mut s, "auto_flange", json!({}))["flanges"], 0, "flanged tabs are left alone");
  assert!(s.design.features.iter().any(|f| f.name == "Bend Lines"));
}
