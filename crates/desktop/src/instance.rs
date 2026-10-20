//! One running app per user, as in Fusion: launching FissionOxide again (from the Start menu, or
//! by opening a `.fox` file) hands the file to the running app, which comes to the front, and the
//! new process exits.
//!
//! The running app holds a lock on `instance.lock` in the [config dir](crate::config_dir) and
//! listens on a loopback port, which it writes with a random token to `instance.json` beside it. A
//! later launch that can't take the lock sends `{"token", "open"}` there. Setting `$FOX_CONFIG_DIR`
//! gives a separate instance (as tests do); `$FOX_MULTI_INSTANCE=1` turns the guard off.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// What a later launch asked the running app to do: come to the front, and open a file if given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
  pub open: Option<PathBuf>,
}

type Wake = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

/// This process as the running app: holds the lock while it lives, and takes later launches'
/// requests.
pub struct Instance {
  _lock: Option<File>,
  rx: mpsc::Receiver<Request>,
  wake: Wake,
}

impl Instance {
  /// An instance that takes no requests (the guard is off, or couldn't be set up).
  fn alone() -> Instance {
    Instance { _lock: None, rx: mpsc::channel().1, wake: Default::default() }
  }

  /// `wake` is called (from another thread) when a request arrives to [`poll`](Instance::poll),
  /// so an idle window can repaint and pick it up.
  pub fn set_wake(&self, wake: impl Fn() + Send + 'static) {
    *self.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(wake));
  }

  /// The requests that arrived since the last call.
  pub fn poll(&self) -> Vec<Request> {
    self.rx.try_iter().collect()
  }
}

/// Whether the guard is turned off by `$FOX_MULTI_INSTANCE`.
pub fn disabled_by_env() -> bool {
  std::env::var("FOX_MULTI_INSTANCE").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Becomes the running app, or hands `open` to the one already running. `None` means it was
/// handed over and this process should exit. If the running app can't be reached, this one runs
/// alongside it.
pub fn claim(open: Option<&Path>) -> Option<Instance> {
  claim_in(&crate::config_dir(), open)
}

fn claim_in(dir: &Path, open: Option<&Path>) -> Option<Instance> {
  let lock = std::fs::create_dir_all(dir)
    .and_then(|_| File::options().read(true).write(true).create(true).truncate(false).open(dir.join("instance.lock")));
  let lock = match lock {
    Ok(f) => f,
    Err(e) => {
      log::warn!("no single-instance guard: {e}");
      return Some(Instance::alone());
    }
  };
  match lock.try_lock() {
    Ok(()) => Some(serve(dir, lock)),
    Err(std::fs::TryLockError::WouldBlock) => {
      let open = open.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
      match forward(dir, open.as_deref()) {
        Ok(()) => None,
        Err(e) => {
          log::warn!("couldn't reach the running FissionOxide ({e}); starting another");
          Some(Instance::alone())
        }
      }
    }
    Err(std::fs::TryLockError::Error(e)) => {
      log::warn!("no single-instance guard: {e}");
      Some(Instance::alone())
    }
  }
}

/// Listens for later launches.
fn serve(dir: &Path, lock: File) -> Instance {
  let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
    Ok(l) => l,
    Err(e) => {
      log::warn!("no single-instance listener: {e}");
      return Instance { _lock: Some(lock), ..Instance::alone() };
    }
  };
  let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
  let token = token();
  let info = json!({"port": port, "token": token, "pid": std::process::id()});
  let tmp = dir.join("instance.json.tmp");
  if let Err(e) = std::fs::write(&tmp, info.to_string()).and_then(|_| std::fs::rename(&tmp, dir.join("instance.json")))
  {
    log::warn!("no single-instance listener: {e}");
    return Instance { _lock: Some(lock), ..Instance::alone() };
  }
  let (tx, rx) = mpsc::channel();
  let wake: Wake = Default::default();
  let thread_wake = wake.clone();
  let spawned = std::thread::Builder::new().name("instance".into()).spawn(move || {
    for stream in listener.incoming().flatten() {
      match receive(stream, &token) {
        Ok(r) => {
          if tx.send(r).is_err() {
            return;
          }
          if let Some(w) = &*thread_wake.lock().unwrap_or_else(|e| e.into_inner()) {
            w();
          }
        }
        Err(e) => log::debug!("ignored a connection to the instance port: {e}"),
      }
    }
  });
  if let Err(e) = spawned {
    log::warn!("no single-instance listener: {e}");
  }
  Instance { _lock: Some(lock), rx, wake }
}

fn receive(stream: TcpStream, token: &str) -> std::io::Result<Request> {
  stream.set_read_timeout(Some(Duration::from_secs(2)))?;
  let mut line = String::new();
  BufReader::new(&stream).take_line(&mut line, 64 * 1024)?;
  let v: Value = serde_json::from_str(&line).map_err(std::io::Error::other)?;
  if v.get("token").and_then(Value::as_str) != Some(token) {
    return Err(std::io::Error::other("wrong token"));
  }
  let open = v.get("open").and_then(Value::as_str).map(PathBuf::from);
  (&stream).write_all(b"ok\n")?;
  Ok(Request { open })
}

/// Sends the request to the running app. Retries briefly, as it may still be starting up (it
/// takes the lock before it writes where it listens).
fn forward(dir: &Path, open: Option<&Path>) -> std::io::Result<()> {
  let start = Instant::now();
  loop {
    match try_forward(dir, open) {
      Ok(()) => return Ok(()),
      Err(e) if start.elapsed() > Duration::from_secs(5) => return Err(e),
      Err(_) => std::thread::sleep(Duration::from_millis(100)),
    }
  }
}

fn try_forward(dir: &Path, open: Option<&Path>) -> std::io::Result<()> {
  let info: Value =
    serde_json::from_slice(&std::fs::read(dir.join("instance.json"))?).map_err(std::io::Error::other)?;
  let port = info.get("port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok());
  let token = info.get("token").and_then(Value::as_str);
  let (Some(port), Some(token)) = (port, token) else {
    return Err(std::io::Error::other("instance.json is incomplete"));
  };
  let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
  let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
  stream.set_read_timeout(Some(Duration::from_secs(2)))?;
  let msg = json!({"token": token, "open": open.map(|p| p.to_string_lossy())});
  stream.write_all(format!("{msg}\n").as_bytes())?;
  let mut reply = String::new();
  BufReader::new(&stream).take_line(&mut reply, 16)?;
  if reply.trim() == "ok" { Ok(()) } else { Err(std::io::Error::other("the running app didn't answer")) }
}

/// A token that's hard to guess, so other users' programs on this computer can't drive the app
/// through its port (only this user can read `instance.json`).
fn token() -> String {
  use std::hash::{BuildHasher, Hasher};
  (0..4)
    .map(|i| {
      let mut h = std::collections::hash_map::RandomState::new().build_hasher();
      h.write_u32(i);
      h.write_u32(std::process::id());
      h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
      format!("{:016x}", h.finish())
    })
    .collect()
}

trait TakeLine {
  /// Reads one line of at most `max` bytes.
  fn take_line(self, buf: &mut String, max: u64) -> std::io::Result<usize>;
}

impl<R: std::io::Read> TakeLine for BufReader<R> {
  fn take_line(self, buf: &mut String, max: u64) -> std::io::Result<usize> {
    use std::io::Read;
    BufReader::new(self.take(max)).read_line(buf)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_second_launch_hands_its_file_to_the_first() {
    let dir = std::env::temp_dir().join(format!("fox-instance-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let first = claim_in(&dir, None).expect("the first launch runs");
    let (woke_tx, woke_rx) = mpsc::channel();
    first.set_wake(move || {
      let _ = woke_tx.send(());
    });

    assert!(claim_in(&dir, Some(Path::new("part.fox"))).is_none(), "the second launch hands over");
    woke_rx.recv_timeout(Duration::from_secs(5)).expect("the first is woken");
    let got = first.poll();
    assert_eq!(got.len(), 1);
    let open = got[0].open.as_ref().expect("a file");
    assert!(open.is_absolute() && open.ends_with("part.fox"), "{open:?}");

    assert!(claim_in(&dir, None).is_none());
    woke_rx.recv_timeout(Duration::from_secs(5)).expect("woken again");
    assert_eq!(first.poll(), vec![Request { open: None }]);

    // a wrong token is turned away
    let info: Value = serde_json::from_slice(&std::fs::read(dir.join("instance.json")).unwrap()).unwrap();
    let port = info["port"].as_u64().unwrap() as u16;
    let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    s.write_all(b"{\"token\": \"nope\", \"open\": \"x.fox\"}\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(first.poll().is_empty());

    // once the first has gone, the next launch runs
    drop(first);
    assert!(claim_in(&dir, None).is_some());
    let _ = std::fs::remove_dir_all(&dir);
  }
}
