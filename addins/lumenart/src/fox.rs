//! Bindings to the FissionOxide plugin host (API 1). Copy this file into a new plugin.
//!
//! The host gives a plugin only these functions; everything a plugin does to a design goes
//! through [`execute`], the same JSON commands as the CLI and the MCP server
//! (`fissionoxide-cli commands` lists them).

use serde_json::{Value, json};

#[link(wasm_import_module = "fox")]
unsafe extern "C" {
  #[link_name = "execute"]
  fn host_execute(cmd: *const u8, cmd_len: usize, params: *const u8, params_len: usize) -> i64;
  #[link_name = "resource"]
  fn host_resource(path: *const u8, path_len: usize) -> i64;
  #[link_name = "storage_get"]
  fn host_storage_get(key: *const u8, key_len: usize) -> i64;
  #[link_name = "storage_set"]
  fn host_storage_set(key: *const u8, key_len: usize, value: *const u8, value_len: isize);
  #[link_name = "write_file"]
  fn host_write_file(path: *const u8, path_len: usize, data: *const u8, data_len: usize) -> i64;
  #[link_name = "progress"]
  fn host_progress(fraction: f32, msg: *const u8, msg_len: usize) -> i32;
  #[link_name = "log"]
  fn host_log(level: i32, msg: *const u8, msg_len: usize);
}

/// The host allocates the strings it hands over with this.
#[unsafe(no_mangle)]
pub extern "C" fn fox_alloc(len: usize) -> *mut u8 {
  let mut v = Vec::<u8>::with_capacity(len.max(1));
  let p = v.as_mut_ptr();
  std::mem::forget(v);
  p
}

/// Bytes the host handed back, packed as `(pointer << 32) | length`; `None` for -1.
fn take(packed: i64) -> Option<Vec<u8>> {
  if packed == -1 {
    return None;
  }
  let (p, n) = ((packed as u64 >> 32) as usize, (packed as u64 & 0xffff_ffff) as usize);
  // SAFETY: the host wrote `n` bytes at `p`, in a block it got from `fox_alloc`.
  Some(unsafe { std::slice::from_raw_parts(p as *const u8, n) }.to_vec())
}

/// A string handed back to the host, packed. The instance is thrown away after each call, so
/// nothing needs freeing.
pub fn give(s: String) -> i64 {
  let s = s.leak();
  (((s.as_ptr() as u64) << 32) | s.len() as u64) as i64
}

/// Reads a string the host passed in.
pub fn arg(p: *const u8, n: usize) -> String {
  // SAFETY: the host passes a (pointer, length) pair it wrote into memory from `fox_alloc`.
  String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }).into_owned()
}

/// Runs an engine command; its JSON result or the error.
pub fn execute(cmd: &str, params: Value) -> Result<Value, String> {
  let params = params.to_string();
  // SAFETY: plain (pointer, length) pairs of live strings.
  let r = unsafe { host_execute(cmd.as_ptr(), cmd.len(), params.as_ptr(), params.len()) };
  let reply: Value = serde_json::from_slice(&take(r).unwrap_or_default()).map_err(|e| e.to_string())?;
  match reply.get("error").and_then(Value::as_str) {
    Some(e) => Err(format!("{cmd}: {e}")),
    None => Ok(reply.get("ok").cloned().unwrap_or(Value::Null)),
  }
}

/// A file from the plugin's `resources/` folder.
pub fn resource(path: &str) -> Option<Vec<u8>> {
  // SAFETY: as above.
  take(unsafe { host_resource(path.as_ptr(), path.len()) })
}

/// Plugin-private storage that survives restarts and updates.
pub fn storage_get(key: &str) -> Option<String> {
  // SAFETY: as above.
  take(unsafe { host_storage_get(key.as_ptr(), key.len()) }).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn storage_set(key: &str, value: &str) {
  // SAFETY: as above.
  unsafe { host_storage_set(key.as_ptr(), key.len(), value.as_ptr(), value.len() as isize) }
}

pub fn storage_remove(key: &str) {
  // SAFETY: as above; a negative length removes the key.
  unsafe { host_storage_set(key.as_ptr(), key.len(), key.as_ptr(), -1) }
}

/// Writes a file into the folder the user picked for the run (needs "write-files").
pub fn write_file(path: &str, data: &[u8]) -> Result<(), String> {
  // SAFETY: as above.
  let r = unsafe { host_write_file(path.as_ptr(), path.len(), data.as_ptr(), data.len()) };
  if r == 0 { Ok(()) } else { Err(String::from_utf8_lossy(&take(r).unwrap_or_default()).into_owned()) }
}

/// Reports progress; false means the user cancelled (return soon, the run is rolled back).
pub fn progress(fraction: f32, msg: &str) -> bool {
  // SAFETY: as above.
  unsafe { host_progress(fraction, msg.as_ptr(), msg.len()) != 0 }
}

/// Writes to the plugin's log (0 error, 1 warning, 2 info, 3 debug).
pub fn log(level: i32, msg: &str) {
  // SAFETY: as above.
  unsafe { host_log(level, msg.as_ptr(), msg.len()) }
}

/// The reply to `fox_run` / `fox_input_changed`.
pub fn reply(r: Result<Value, String>) -> i64 {
  give(match r {
    Ok(v) => json!({"ok": v}).to_string(),
    Err(e) => json!({"error": e}).to_string(),
  })
}
