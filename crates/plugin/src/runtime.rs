//! Running plugin code: a WebAssembly module on Wasmtime that can only reach the host functions
//! below. Each call gets a fresh instance, a fuel budget (so an endless loop ends) and a memory
//! cap; a trap ends the call with an error.
//!
//! The ABI (see docs/PLUGINS.md): strings are UTF-8 (pointer, length) pairs in the module's
//! memory. Buffers the host hands back are allocated with the module's `fox_alloc` and returned
//! packed as `(pointer << 32) | length` in an `i64`; `-1` means "none".

use std::path::PathBuf;
use std::sync::OnceLock;

use fox_engine::Session;
use serde_json::{Map, Value, json};
use wasmtime::{Caller, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap, ValType};

use crate::sandbox::Context;

/// Instructions a call may run (roughly; a few seconds of work).
pub const DEFAULT_FUEL: u64 = 20_000_000_000;
/// The most memory a plugin instance may grow to.
pub const MAX_MEMORY: usize = 1 << 30;
/// The most a plugin may store with `storage_set`.
const MAX_STORAGE: usize = 16 << 20;

/// Host functions a module may import from `fox`: (name, parameters, results).
const IMPORTS: [(&str, &[ValKind], &[ValKind]); 7] = [
  ("execute", &[ValKind::I32; 4], &[ValKind::I64]),
  ("resource", &[ValKind::I32; 2], &[ValKind::I64]),
  ("storage_get", &[ValKind::I32; 2], &[ValKind::I64]),
  ("storage_set", &[ValKind::I32; 4], &[]),
  ("write_file", &[ValKind::I32; 4], &[ValKind::I64]),
  ("progress", &[ValKind::F32, ValKind::I32, ValKind::I32], &[ValKind::I32]),
  ("log", &[ValKind::I32; 3], &[]),
];

/// Functions a module exports: (name, parameters, results, required).
const EXPORTS: [(&str, &[ValKind], &[ValKind], bool); 3] = [
  ("fox_alloc", &[ValKind::I32], &[ValKind::I32], true),
  ("fox_run", &[ValKind::I32; 4], &[ValKind::I64], true),
  ("fox_input_changed", &[ValKind::I32; 6], &[ValKind::I64], false),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum ValKind {
  I32,
  I64,
  F32,
  Other,
}

fn kind(v: &ValType) -> ValKind {
  match v {
    ValType::I32 => ValKind::I32,
    ValType::I64 => ValKind::I64,
    ValType::F32 => ValKind::F32,
    _ => ValKind::Other,
  }
}

pub fn engine() -> &'static Engine {
  static ENGINE: OnceLock<Engine> = OnceLock::new();
  ENGINE.get_or_init(|| {
    let mut c = wasmtime::Config::new();
    c.consume_fuel(true);
    // OpenCASCADE installs its own SIGSEGV/SIGILL handlers, which would take wasm traps for
    // crashes of the app: compile explicit bounds checks and traps instead
    c.signals_based_traps(false);
    Engine::new(&c).expect("the plugin engine configuration is valid")
  })
}

/// Compiles a module and checks it imports only host functions that exist and exports what the
/// host calls.
pub fn compile(wasm: &[u8]) -> Result<Module, String> {
  let m = Module::new(engine(), wasm).map_err(|e| format!("plugin.wasm doesn't compile: {e:#}"))?;
  for imp in m.imports() {
    let (module, name) = (imp.module(), imp.name());
    let found = IMPORTS.iter().find(|(n, _, _)| module == "fox" && *n == name);
    let Some((_, params, results)) = found else {
      return Err(format!("plugin.wasm imports '{module}.{name}', which plugins don't have"));
    };
    let ok = imp.ty().func().is_some_and(|f| {
      f.params().map(|p| kind(&p)).eq(params.iter().copied()) && f.results().map(|r| kind(&r)).eq(results.iter().copied())
    });
    if !ok {
      return Err(format!("plugin.wasm imports 'fox.{name}' with the wrong signature"));
    }
  }
  if !m.exports().any(|e| e.name() == "memory" && e.ty().memory().is_some()) {
    return Err("plugin.wasm doesn't export its memory".into());
  }
  for (name, params, results, required) in EXPORTS {
    match m.get_export(name) {
      None if required => return Err(format!("plugin.wasm doesn't export '{name}'")),
      None => {}
      Some(e) => {
        let ok = e.func().is_some_and(|f| {
          f.params().map(|p| kind(&p)).eq(params.iter().copied())
            && f.results().map(|r| kind(&r)).eq(results.iter().copied())
        });
        if !ok {
          return Err(format!("plugin.wasm exports '{name}' with the wrong signature"));
        }
      }
    }
  }
  Ok(m)
}

pub fn check_module(wasm: &[u8]) -> Result<(), String> {
  compile(wasm).map(|_| ())
}

/// A log line from a plugin.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LogLine {
  /// 0 error, 1 warning, 2 info, 3 debug.
  pub level: u8,
  pub message: String,
}

/// Progress callback: (fraction 0..1, message) → keep going.
pub type Progress = Box<dyn FnMut(f32, &str) -> bool>;

/// Everything a running plugin can reach.
pub struct Host {
  pub session: Session,
  pub ctx: Context,
  /// The plugin's key/value store (`storage.json` beside its installed versions).
  pub storage_path: PathBuf,
  storage: Option<Map<String, Value>>,
  storage_dirty: bool,
  pub log: Vec<LogLine>,
  pub progress: Option<Progress>,
  pub cancelled: bool,
  /// Files the plugin wrote with `write_file`.
  pub written: Vec<PathBuf>,
  /// Engine commands it ran.
  pub commands: usize,
  limits: StoreLimits,
}

impl Host {
  pub fn new(session: Session, ctx: Context, storage_path: PathBuf) -> Host {
    Host {
      session,
      ctx,
      storage_path,
      storage: None,
      storage_dirty: false,
      log: Vec::new(),
      progress: None,
      cancelled: false,
      written: Vec::new(),
      commands: 0,
      limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY).instances(1).tables(4).memories(1).build(),
    }
  }

  fn storage(&mut self) -> &mut Map<String, Value> {
    let path = &self.storage_path;
    self.storage.get_or_insert_with(|| {
      std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    })
  }

  /// Writes the store back if the plugin changed it.
  pub fn save_storage(&mut self) -> Result<(), String> {
    if !self.storage_dirty {
      return Ok(());
    }
    let text = Value::Object(self.storage().clone()).to_string();
    if let Some(dir) = self.storage_path.parent() {
      std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&self.storage_path, text).map_err(|e| e.to_string())?;
    self.storage_dirty = false;
    Ok(())
  }

  fn execute(&mut self, cmd: &str, params: &str) -> Value {
    let params: Value = match serde_json::from_str(params) {
      Ok(p) => p,
      Err(e) => return json!({"error": format!("bad JSON parameters for '{cmd}': {e}")}),
    };
    let params = if params.is_null() { json!({}) } else { params };
    let params = match self.ctx.prepare(cmd, &params) {
      Ok(p) => p,
      Err(e) => return json!({"error": e}),
    };
    self.commands += 1;
    match self.session.execute(cmd, &params) {
      Ok(v) => match v.get("error").and_then(Value::as_str) {
        Some(e) => json!({"error": e}),
        None => json!({"ok": v}),
      },
      Err(e) => json!({"error": e.0}),
    }
  }
}

/// What to call.
pub enum Entry<'a> {
  /// `fox_run(command, inputs)`.
  Run { command: &'a str, inputs: &'a Value },
  /// `fox_input_changed(command, input, inputs)`.
  InputChanged { command: &'a str, input: &'a str, inputs: &'a Value },
}

/// Calls into a module. Returns the host (with the session) and the plugin's reply.
pub fn invoke(module: &Module, host: Host, entry: Entry<'_>, fuel: u64) -> (Host, Result<Value, String>) {
  let mut store = Store::new(engine(), host);
  store.limiter(|h| &mut h.limits);
  let result = call(&mut store, module, entry, fuel);
  let mut host = store.into_data();
  let result = result.and_then(|v| host.save_storage().map(|_| v));
  (host, result)
}

fn call(store: &mut Store<Host>, module: &Module, entry: Entry<'_>, fuel: u64) -> Result<Value, String> {
  store.set_fuel(fuel).map_err(|e| e.to_string())?;
  let linker = linker().map_err(|e| e.to_string())?;
  let instance = linker.instantiate(&mut *store, module).map_err(|e| trap_message(store, e))?;
  let reply = match entry {
    Entry::Run { command, inputs } => {
      let f = instance.get_typed_func::<(i32, i32, i32, i32), i64>(&mut *store, "fox_run").map_err(|e| e.to_string())?;
      let (a, b) = pass(store, &instance, command.as_bytes())?;
      let (c, d) = pass(store, &instance, inputs.to_string().as_bytes())?;
      f.call(&mut *store, (a, b, c, d)).map_err(|e| trap_message(store, e))?
    }
    Entry::InputChanged { command, input, inputs } => {
      let Ok(f) =
        instance.get_typed_func::<(i32, i32, i32, i32, i32, i32), i64>(&mut *store, "fox_input_changed")
      else {
        return Ok(json!({}));
      };
      let (a, b) = pass(store, &instance, command.as_bytes())?;
      let (c, d) = pass(store, &instance, input.as_bytes())?;
      let (e, g) = pass(store, &instance, inputs.to_string().as_bytes())?;
      f.call(&mut *store, (a, b, c, d, e, g)).map_err(|e| trap_message(store, e))?
    }
  };
  if store.data().cancelled {
    return Err("cancelled".into());
  }
  let memory = instance.get_memory(&mut *store, "memory").ok_or("plugin.wasm has no memory")?;
  let (ptr, len) = unpack(reply);
  let bytes = memory
    .data(&*store)
    .get(ptr..ptr.saturating_add(len))
    .ok_or("the plugin returned a reply outside its memory")?;
  let reply: Value =
    serde_json::from_slice(bytes).map_err(|e| format!("the plugin's reply is not JSON ({e})"))?;
  match (reply.get("ok"), reply.get("error")) {
    (_, Some(Value::String(e))) => Err(e.clone()),
    (Some(v), _) => Ok(v.clone()),
    _ => Err("the plugin's reply has neither \"ok\" nor \"error\"".into()),
  }
}

fn trap_message(store: &Store<Host>, e: wasmtime::Error) -> String {
  if store.data().cancelled {
    return "cancelled".into();
  }
  match e.downcast_ref::<Trap>() {
    Some(Trap::OutOfFuel) => "the plugin ran too long and was stopped".into(),
    Some(t) => format!("the plugin crashed: {t}"),
    None => format!("the plugin failed: {e:#}"),
  }
}

fn unpack(v: i64) -> (usize, usize) {
  let v = v as u64;
  ((v >> 32) as usize, (v & 0xffff_ffff) as usize)
}

fn packed(ptr: i32, len: usize) -> i64 {
  (((ptr as u32 as u64) << 32) | len as u64) as i64
}

/// Copies bytes into the module's memory (allocated by `fox_alloc`).
fn pass(store: &mut Store<Host>, instance: &wasmtime::Instance, data: &[u8]) -> Result<(i32, i32), String> {
  let alloc = instance.get_typed_func::<i32, i32>(&mut *store, "fox_alloc").map_err(|e| e.to_string())?;
  let len = i32::try_from(data.len()).map_err(|_| "too much data for the plugin")?;
  let ptr = alloc.call(&mut *store, len).map_err(|e| trap_message(store, e))?;
  let memory = instance.get_memory(&mut *store, "memory").ok_or("plugin.wasm has no memory")?;
  memory.write(&mut *store, ptr as u32 as usize, data).map_err(|_| "fox_alloc returned memory out of bounds")?;
  Ok((ptr, len))
}

fn read(caller: &mut Caller<'_, Host>, ptr: i32, len: i32) -> wasmtime::Result<Vec<u8>> {
  let mem = match caller.get_export("memory") {
    Some(Extern::Memory(m)) => m,
    _ => return Err(wasmtime::Error::msg("plugin.wasm has no memory")),
  };
  let (p, l) = (ptr as u32 as usize, len as u32 as usize);
  mem
    .data(&*caller)
    .get(p..p.saturating_add(l))
    .map(<[u8]>::to_vec)
    .ok_or_else(|| wasmtime::Error::msg("the plugin passed memory outside its bounds"))
}

fn read_str(caller: &mut Caller<'_, Host>, ptr: i32, len: i32) -> wasmtime::Result<String> {
  String::from_utf8(read(caller, ptr, len)?).map_err(|_| wasmtime::Error::msg("the plugin passed a string that isn't UTF-8"))
}

/// Hands bytes to the module: allocated with `fox_alloc`, returned packed.
fn give(caller: &mut Caller<'_, Host>, data: &[u8]) -> wasmtime::Result<i64> {
  let alloc = match caller.get_export("fox_alloc") {
    Some(Extern::Func(f)) => f.typed::<i32, i32>(&*caller)?,
    _ => return Err(wasmtime::Error::msg("plugin.wasm doesn't export fox_alloc")),
  };
  let len = i32::try_from(data.len()).map_err(|_| wasmtime::Error::msg("too much data for the plugin"))?;
  let ptr = alloc.call(&mut *caller, len)?;
  let mem = match caller.get_export("memory") {
    Some(Extern::Memory(m)) => m,
    _ => return Err(wasmtime::Error::msg("plugin.wasm has no memory")),
  };
  mem.write(&mut *caller, ptr as u32 as usize, data)?;
  Ok(packed(ptr, data.len()))
}

fn linker() -> wasmtime::Result<Linker<Host>> {
  let mut l = Linker::new(engine());
  l.func_wrap("fox", "execute", |mut c: Caller<'_, Host>, cp: i32, cl: i32, pp: i32, pl: i32| {
    let cmd = read_str(&mut c, cp, cl)?;
    let params = read_str(&mut c, pp, pl)?;
    let reply = c.data_mut().execute(&cmd, &params);
    give(&mut c, reply.to_string().as_bytes())
  })?;
  l.func_wrap("fox", "resource", |mut c: Caller<'_, Host>, pp: i32, pl: i32| {
    let rel = read_str(&mut c, pp, pl)?;
    let data = c.data().ctx.resource(&rel).ok().and_then(|p| std::fs::read(p).ok());
    match data {
      Some(d) => give(&mut c, &d),
      None => Ok(-1),
    }
  })?;
  l.func_wrap("fox", "storage_get", |mut c: Caller<'_, Host>, kp: i32, kl: i32| {
    let key = read_str(&mut c, kp, kl)?;
    match c.data_mut().storage().get(&key).and_then(Value::as_str).map(str::to_string) {
      Some(v) => give(&mut c, v.as_bytes()),
      None => Ok(-1),
    }
  })?;
  l.func_wrap("fox", "storage_set", |mut c: Caller<'_, Host>, kp: i32, kl: i32, vp: i32, vl: i32| {
    let key = read_str(&mut c, kp, kl)?;
    let value = if vl < 0 { None } else { Some(read_str(&mut c, vp, vl)?) };
    let h = c.data_mut();
    let st = h.storage();
    match value {
      Some(v) => {
        st.insert(key, Value::String(v));
      }
      None => {
        st.remove(&key);
      }
    }
    if Value::Object(st.clone()).to_string().len() > MAX_STORAGE {
      return Err(wasmtime::Error::msg("the plugin's storage is full"));
    }
    h.storage_dirty = true;
    Ok(())
  })?;
  l.func_wrap("fox", "write_file", |mut c: Caller<'_, Host>, pp: i32, pl: i32, dp: i32, dl: i32| {
    let rel = read_str(&mut c, pp, pl)?;
    let data = read(&mut c, dp, dl)?;
    let h = c.data_mut();
    let result = if h.ctx.granted.contains(&crate::Permission::WriteFiles) {
      h.ctx.in_folder(&rel, true).and_then(|p| match std::fs::write(&p, &data) {
        Ok(()) => {
          h.written.push(p);
          Ok(())
        }
        Err(e) => Err(format!("{rel}: {e}")),
      })
    } else {
      Err("writing files needs the 'write-files' permission, which this plugin was not given".into())
    };
    match result {
      Ok(()) => Ok(0),
      Err(e) => give(&mut c, e.as_bytes()),
    }
  })?;
  l.func_wrap("fox", "progress", |mut c: Caller<'_, Host>, fraction: f32, mp: i32, ml: i32| {
    let msg = read_str(&mut c, mp, ml)?;
    let h = c.data_mut();
    let go = match h.progress.as_mut() {
      Some(f) => f(fraction.clamp(0.0, 1.0), &msg),
      None => true,
    };
    if !go {
      h.cancelled = true;
    }
    Ok(i32::from(go))
  })?;
  l.func_wrap("fox", "log", |mut c: Caller<'_, Host>, level: i32, mp: i32, ml: i32| {
    let message = read_str(&mut c, mp, ml)?;
    let h = c.data_mut();
    if h.log.len() < 10_000 {
      h.log.push(LogLine { level: level.clamp(0, 3) as u8, message });
    }
    Ok(())
  })?;
  Ok(l)
}
