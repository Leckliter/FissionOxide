//! A hub on a hub server (`fox-hub-server`), reached at an `http://` or `https://` URL.
//!
//! The store keeps a mirror of the server's index, so reading (listings, versions, activity,
//! search) works as for a local hub; every change is sent as one [`Op`] and the mirror is
//! brought up to date after it. Version files and thumbnails are fetched when first read and
//! cached (they never change). A WebSocket carries presence and "the index changed" in both
//! directions in real time; while it is down, presence goes over plain HTTP and the index is
//! re-checked on every use.
//!
//! The access token comes from the URL (`https://TOKEN@hub.example.com`), the caller, the
//! `FOX_HUB_TOKEN` environment variable, or the one remembered for the URL with
//! [`remember_token`].
//!
//! The protocol, all JSON with `Authorization: Bearer TOKEN`:
//!
//! ```text
//! GET  /api/whoami                     {user, server}
//! GET  /api/index?rev=N                {rev, index} (or {rev, unchanged: true} if N is current)
//! POST /api/op {session, host, op}     {result, rev}, or an error status with {error, kind}
//! GET  /api/files/ITEM/FILE            a version's file, thumbnail or original upload
//! GET  /api/presence                   [Presence]
//! POST /api/presence Here              {}  (a heartbeat: lasts presence::TTL seconds)
//! GET  /api/ws                         WebSocket: the client sends {"type": "here", ...Here};
//!                                      the server {"type": "changed", rev} and
//!                                      {"type": "presence", presence: [Presence]}
//! ```

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::ops::Op;
use crate::presence::{Here, Presence};
use crate::{Index, Result, StoreError, host, session_id};

/// The hub server URL `root` names, without a token in it, if it is one.
pub fn url_of(root: &Path) -> Option<String> {
  let s = root.to_string_lossy();
  is_url(&s).then(|| parse(&s).map(|(base, _)| base).unwrap_or_else(|_| s.trim_end_matches('/').to_string()))
}

/// Whether `root` names a hub server rather than a directory.
pub fn is_remote(root: &Path) -> bool {
  is_url(&root.to_string_lossy())
}

fn is_url(s: &str) -> bool {
  let l = s.trim().to_ascii_lowercase();
  l.starts_with("http://") || l.starts_with("https://")
}

/// Bumped whenever a hub server tells this process something changed (its index, or who has
/// what open): a UI can re-read the hub when it moves.
pub fn generation() -> u64 {
  GENERATION.load(Ordering::Relaxed)
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

fn bump() {
  GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Splits `https://TOKEN@host:port/path/` into `https://host:port/path` and the token.
fn parse(url: &str) -> Result<(String, Option<String>)> {
  let url = url.trim();
  let (scheme, rest) = url.split_once("://").ok_or_else(|| StoreError::Remote(format!("'{url}' isn't a URL")))?;
  let (auth, path) = match rest.find('/') {
    Some(i) => (&rest[..i], &rest[i..]),
    None => (rest, ""),
  };
  let (token, hostport) = match auth.rsplit_once('@') {
    // `user:token@host` or `token@host`
    Some((info, h)) => (Some(info.rsplit_once(':').map(|(_, t)| t).unwrap_or(info).to_string()), h),
    None => (None, auth),
  };
  if hostport.is_empty() {
    return Err(StoreError::Remote(format!("'{url}' names no host")));
  }
  let base = format!("{}://{hostport}{}", scheme.to_ascii_lowercase(), path.trim_end_matches('/'));
  Ok((base, token.filter(|t| !t.is_empty())))
}

/// Where hub server tokens and caches are kept: beside the local hub, never in a hub server.
fn local_dir() -> PathBuf {
  let root = crate::Store::default_root();
  if !is_remote(&root) {
    return root.join("remote");
  }
  if crate::is_test_process() {
    return crate::test_process_root().join("remote");
  }
  crate::platform_dir("FissionOxide", "fissionoxide").join("remote")
}

fn cache_of(base: &str) -> PathBuf {
  let safe: String = base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect();
  local_dir().join(safe)
}

/// The local directory kept for the hub server `root` names (its file cache, and what is kept
/// beside a hub, like tool libraries), if it is one.
pub fn local_dir_for(root: &Path) -> Option<PathBuf> {
  url_of(root).map(|u| cache_of(&u))
}

fn tokens_file() -> PathBuf {
  local_dir().join("tokens.json")
}

fn read_tokens() -> HashMap<String, String> {
  std::fs::read_to_string(tokens_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

static SESSION_TOKENS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Uses `token` for the hub server at `url` in this process (without remembering it).
pub fn use_token(url: &str, token: &str) {
  let Ok((base, _)) = parse(url) else { return };
  let mut t = SESSION_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
  t.retain(|(u, _)| *u != base);
  t.push((base, token.to_string()));
}

/// The token remembered for the hub server at `url`.
pub fn token_for(url: &str) -> Option<String> {
  let base = parse(url).ok()?.0;
  read_tokens().remove(&base)
}

/// Remembers `token` for the hub server at `url` (readable only by this user where the file
/// system can say so).
pub fn remember_token(url: &str, token: &str) -> Result<()> {
  let base = parse(url)?.0;
  let mut all = read_tokens();
  all.insert(base, token.to_string());
  let path = tokens_file();
  if let Some(d) = path.parent() {
    std::fs::create_dir_all(d)?;
  }
  std::fs::write(&path, serde_json::to_string_pretty(&all)?)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
  }
  Ok(())
}

/// Remembers the hub server the app last connected to (`None`: back to the local hub).
pub fn remember_last(url: Option<&str>) -> Result<()> {
  let path = local_dir().join("last.txt");
  match url {
    Some(u) => {
      std::fs::create_dir_all(local_dir())?;
      std::fs::write(path, parse(u)?.0)?;
    }
    None => {
      let _ = std::fs::remove_file(path);
    }
  }
  Ok(())
}

/// The hub server the app last connected to, if it didn't go back to the local hub since.
pub fn last_server() -> Option<String> {
  let s = std::fs::read_to_string(local_dir().join("last.txt")).ok()?;
  let s = s.trim();
  is_url(s).then(|| s.to_string())
}

/// One connection to a hub server, shared by every store this process opens on it.
pub struct Conn {
  /// `https://host[:port][/path]`, no token.
  pub url: String,
  token: Option<String>,
  agent: ureq::Agent,
  /// Who the token belongs to.
  pub user: String,
  state: Mutex<State>,
}

#[derive(Default)]
struct State {
  /// The mirrored index and the server revision it is at.
  index: Option<Index>,
  rev: u64,
  /// The newest revision the server has announced.
  announced: u64,
  /// The WebSocket is up.
  ws: bool,
  presence: Vec<Presence>,
  /// What this process has open, and whether the WebSocket still has to be told.
  here: Option<Here>,
  here_unsent: bool,
}

static CONNS: Mutex<Vec<Arc<Conn>>> = Mutex::new(Vec::new());

/// Whether this process has a live WebSocket to the hub server `root` names.
pub fn is_live(root: &Path) -> bool {
  let Some(url) = url_of(root) else { return false };
  CONNS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|c| c.url == url && c.live())
}

/// The connection to the hub server at `url`, made the first time (which checks the token).
pub(crate) fn connect(url: &str, token: Option<&str>) -> Result<Arc<Conn>> {
  let (base, in_url) = parse(url)?;
  let token = token
    .map(str::to_string)
    .or(in_url)
    .or_else(|| {
      let t = SESSION_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
      t.iter().find(|(u, _)| *u == base).map(|(_, t)| t.clone())
    })
    .or_else(|| std::env::var("FOX_HUB_TOKEN").ok().filter(|t| !t.is_empty()))
    .or_else(|| token_for(&base));
  let mut conns = CONNS.lock().unwrap_or_else(|e| e.into_inner());
  if let Some(c) = conns.iter().find(|c| c.url == base && c.token == token) {
    return Ok(c.clone());
  }
  let mut conn = Conn { url: base.clone(), token, agent: agent(&base), user: String::new(), state: Default::default() };
  let who = conn.get_json("/api/whoami")?;
  conn.user = who.get("user").and_then(Value::as_str).unwrap_or("?").to_string();
  let conn = Arc::new(conn);
  conns.push(conn.clone());
  let weak = Arc::downgrade(&conn);
  let _ = std::thread::Builder::new().name("fox-hub-ws".into()).spawn(move || ws_loop(weak));
  Ok(conn)
}

fn agent(url: &str) -> ureq::Agent {
  let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
  let mut cfg = ureq::Agent::config_builder()
    .tls_config(tls)
    .http_status_as_error(false)
    .user_agent(format!("FissionOxide/{}", env!("CARGO_PKG_VERSION")))
    .timeout_connect(Some(Duration::from_secs(10)))
    .timeout_global(Some(Duration::from_secs(600)));
  // a proxy from the environment is no use for a server on this machine
  if host_port(url).is_ok_and(|(h, _, _)| is_loopback(&h)) {
    cfg = cfg.proxy(None);
  }
  cfg.build().new_agent()
}

fn is_loopback(host: &str) -> bool {
  let h = host.trim_start_matches('[').trim_end_matches(']');
  h.eq_ignore_ascii_case("localhost") || h.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Host, port and whether it is TLS, of a base URL.
fn host_port(url: &str) -> Result<(String, u16, bool)> {
  let (scheme, rest) = url.split_once("://").ok_or_else(|| StoreError::Remote(format!("'{url}' isn't a URL")))?;
  let tls = scheme.eq_ignore_ascii_case("https");
  let auth = rest.split('/').next().unwrap_or_default();
  let (h, port) = match auth.rsplit_once(':') {
    Some((h, p)) if !p.contains(']') => {
      (h.to_string(), p.parse().map_err(|_| StoreError::Remote(format!("bad port in '{url}'")))?)
    }
    _ => (auth.to_string(), if tls { 443 } else { 80 }),
  };
  Ok((h, port, tls))
}

fn lock(m: &Mutex<State>) -> MutexGuard<'_, State> {
  m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Conn {
  fn state(&self) -> MutexGuard<'_, State> {
    lock(&self.state)
  }

  fn auth(&self) -> Option<String> {
    self.token.as_ref().map(|t| format!("Bearer {t}"))
  }

  /// Turns an answer into its body, or the error it reports.
  fn check(&self, mut resp: ureq::http::Response<ureq::Body>) -> Result<Vec<u8>> {
    let status = resp.status().as_u16();
    let body =
      resp.body_mut().with_config().limit(u64::MAX).read_to_vec().map_err(|e| self.unreachable(&e.to_string()))?;
    if status < 300 {
      return Ok(body);
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let msg = v.get("error").and_then(Value::as_str).map(str::to_string);
    Err(match (status, v.get("kind").and_then(Value::as_str)) {
      (401, _) if self.token.is_none() => StoreError::Remote(format!(
        "the hub server at {} needs an access token (--token, $FOX_HUB_TOKEN, or Connect in the data panel)",
        self.url
      )),
      (401 | 403, _) => StoreError::Remote(format!("the hub server at {} refused the access token", self.url)),
      (_, Some("not_found")) => StoreError::NotFound(msg.unwrap_or_default()),
      (_, Some("invalid")) => StoreError::Invalid(msg.unwrap_or_default()),
      (_, Some("locked")) => StoreError::Locked(msg.unwrap_or_default()),
      _ => StoreError::Remote(format!(
        "the hub server at {} answered {status}{}",
        self.url,
        msg.map(|m| format!(": {m}")).unwrap_or_default()
      )),
    })
  }

  fn unreachable(&self, e: &str) -> StoreError {
    StoreError::Remote(format!("couldn't reach the hub server at {} ({e})", self.url))
  }

  fn get(&self, path: &str) -> Result<Vec<u8>> {
    let mut req = self.agent.get(format!("{}{path}", self.url));
    if let Some(a) = self.auth() {
      req = req.header("Authorization", a);
    }
    let resp = req.call().map_err(|e| self.unreachable(&e.to_string()))?;
    self.check(resp)
  }

  fn get_json(&self, path: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&self.get(path)?)?)
  }

  fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
    let mut req = self.agent.post(format!("{}{path}", self.url)).header("Content-Type", "application/json");
    if let Some(a) = self.auth() {
      req = req.header("Authorization", a);
    }
    let resp = req.send(serde_json::to_vec(body)?).map_err(|e| self.unreachable(&e.to_string()))?;
    Ok(serde_json::from_slice(&self.check(resp)?)?)
  }

  /// Where this server's files are cached.
  pub(crate) fn cache_dir(&self) -> PathBuf {
    cache_of(&self.url)
  }

  /// The server's index, as it is now.
  pub(crate) fn index(&self) -> Result<Index> {
    let known = {
      let st = self.state();
      match &st.index {
        // the WebSocket would have said if it changed
        Some(i) if st.ws && st.announced <= st.rev => return Ok(i.clone()),
        Some(_) => st.rev,
        None => 0,
      }
    };
    let v = self.get_json(&format!("/api/index?rev={known}"))?;
    let rev = v.get("rev").and_then(Value::as_u64).unwrap_or(0);
    let mut st = self.state();
    if v.get("unchanged").and_then(Value::as_bool) == Some(true)
      && let Some(i) = st.index.clone()
    {
      st.announced = st.announced.max(rev);
      return Ok(i);
    }
    let index: Index = serde_json::from_value(v.get("index").cloned().unwrap_or(Value::Null))
      .map_err(|e| StoreError::Remote(format!("the hub server sent an index that doesn't read: {e}")))?;
    st.index = Some(index.clone());
    st.rev = rev;
    st.announced = st.announced.max(rev);
    Ok(index)
  }

  /// Carries out `op` on the server; its result.
  pub(crate) fn op(&self, op: &Op, session: &str, host: &str) -> Result<Value> {
    let v = self.post_json("/api/op", &json!({"session": session, "host": host, "op": op}))?;
    if let Some(r) = v.get("rev").and_then(Value::as_u64) {
      let mut st = self.state();
      st.announced = st.announced.max(r);
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
  }

  /// A file of an item, fetched into the cache the first time.
  pub(crate) fn cached_file(&self, item: &str, file: &str) -> Result<PathBuf> {
    let safe = |s: &str| !s.is_empty() && !s.starts_with('.') && !s.contains(['/', '\\', ':']);
    if !safe(item) || !safe(file) {
      return Err(StoreError::Invalid(format!("bad file name '{item}/{file}'")));
    }
    let path = self.cache_dir().join("items").join(item).join(file);
    if path.is_file() {
      return Ok(path);
    }
    let bytes = self.get(&format!("/api/files/{item}/{file}"))?;
    if let Some(d) = path.parent() {
      std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension(format!("{}.part", crate::sync::writer_tag()));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
  }

  /// Says what this process has open (see [`crate::Store::here`]).
  pub(crate) fn here(&self, items: &[String]) -> Result<()> {
    let here = {
      let mut st = self.state();
      let h = Here::update(st.here.as_ref(), &self.user, session_id(), &host(), items);
      st.here = Some(h.clone());
      st.here_unsent = true;
      if st.ws {
        return Ok(());
      }
      h
    };
    self.post_json("/api/presence", &serde_json::to_value(&here)?)?;
    self.state().here_unsent = false;
    Ok(())
  }

  /// Who has what open on the server.
  pub(crate) fn presence(&self) -> Result<Vec<Presence>> {
    {
      let st = self.state();
      if st.ws {
        return Ok(st.presence.clone());
      }
    }
    let v = self.get_json("/api/presence")?;
    let list: Vec<Presence> = serde_json::from_value(v)?;
    self.state().presence = list.clone();
    Ok(list)
  }

  /// Whether the WebSocket is up (presence and changes arrive as they happen).
  pub fn live(&self) -> bool {
    self.state().ws
  }

  fn on_event(&self, text: &str) {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return };
    let mut st = self.state();
    if let Some(r) = v.get("rev").and_then(Value::as_u64) {
      st.announced = st.announced.max(r);
    }
    if let Some(p) = v.get("presence").and_then(|p| serde_json::from_value::<Vec<Presence>>(p.clone()).ok()) {
      st.presence = p;
    }
    bump();
  }
}

// ---- the WebSocket ----

enum Stream {
  Plain(TcpStream),
  Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Stream {
  fn tcp(&self) -> &TcpStream {
    match self {
      Stream::Plain(s) => s,
      Stream::Tls(s) => &s.sock,
    }
  }
}

impl Read for Stream {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    match self {
      Stream::Plain(s) => s.read(buf),
      Stream::Tls(s) => s.read(buf),
    }
  }
}

impl Write for Stream {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    match self {
      Stream::Plain(s) => s.write(buf),
      Stream::Tls(s) => s.write(buf),
    }
  }

  fn flush(&mut self) -> std::io::Result<()> {
    match self {
      Stream::Plain(s) => s.flush(),
      Stream::Tls(s) => s.flush(),
    }
  }
}

type Socket = tungstenite::WebSocket<Stream>;

/// How long a read waits before the loop looks for something to send.
const POLL: Duration = Duration::from_millis(100);
const PING: Duration = Duration::from_secs(20);

/// Keeps the WebSocket up while the connection lives, reconnecting with a growing pause.
fn ws_loop(conn: Weak<Conn>) {
  let mut pause = Duration::from_millis(500);
  loop {
    let Some(c) = conn.upgrade() else { return };
    if let Ok(mut ws) = c.ws_connect() {
      pause = Duration::from_millis(500);
      c.run_ws(&mut ws);
    }
    drop(c);
    std::thread::sleep(pause);
    pause = (pause * 2).min(Duration::from_secs(30));
  }
}

impl Conn {
  fn ws_connect(&self) -> std::result::Result<Socket, String> {
    use tungstenite::client::IntoClientRequest;
    let (host, port, tls) = host_port(&self.url).map_err(|e| e.to_string())?;
    let path = self.url.split_once("://").and_then(|(_, r)| r.find('/').map(|i| &r[i..])).unwrap_or("");
    let url = format!("{}://{host}:{port}{path}/api/ws", if tls { "wss" } else { "ws" });
    let mut req = url.into_client_request().map_err(|e| e.to_string())?;
    if let Some(a) = self.auth() {
      req.headers_mut().insert("Authorization", a.parse().map_err(|_| "bad token".to_string())?);
    }
    let addr = (host.trim_start_matches('[').trim_end_matches(']'), port)
      .to_socket_addrs()
      .map_err(|e| e.to_string())?
      .next()
      .ok_or("no address")?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    tcp.set_read_timeout(Some(Duration::from_secs(15))).map_err(|e| e.to_string())?;
    let _ = tcp.set_nodelay(true);
    let stream = if tls {
      use rustls_platform_verifier::ConfigVerifierExt;
      let config = rustls::ClientConfig::with_platform_verifier().map_err(|e| e.to_string())?;
      let name = rustls::pki_types::ServerName::try_from(host.trim_start_matches('[').trim_end_matches(']').to_string())
        .map_err(|e| e.to_string())?;
      let c = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
      Stream::Tls(Box::new(rustls::StreamOwned::new(c, tcp)))
    } else {
      Stream::Plain(tcp)
    };
    let (ws, _) = tungstenite::client(req, stream).map_err(|e| e.to_string())?;
    ws.get_ref().tcp().set_read_timeout(Some(POLL)).map_err(|e| e.to_string())?;
    Ok(ws)
  }

  fn run_ws(&self, ws: &mut Socket) {
    {
      let mut st = self.state();
      st.ws = true;
      // a new connection knows nothing of us yet
      st.here_unsent = st.here.is_some();
    }
    bump();
    let mut pinged = Instant::now();
    loop {
      let send = {
        let mut st = self.state();
        let h = st.here.clone().filter(|_| st.here_unsent);
        st.here_unsent = false;
        h
      };
      if let Some(h) = send {
        let mut msg = serde_json::to_value(&h).unwrap_or(Value::Null);
        msg["type"] = json!("here");
        if ws.send(tungstenite::Message::text(msg.to_string())).is_err() {
          break;
        }
      }
      if pinged.elapsed() > PING {
        pinged = Instant::now();
        if ws.send(tungstenite::Message::Ping(Vec::new().into())).is_err() {
          break;
        }
      }
      match ws.read() {
        Ok(tungstenite::Message::Text(t)) => self.on_event(t.as_str()),
        Ok(tungstenite::Message::Close(_)) => break,
        Ok(_) => {}
        Err(tungstenite::Error::Io(e))
          if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
        Err(_) => break,
      }
    }
    self.state().ws = false;
    bump();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn urls() {
    assert_eq!(parse("https://hub.example.com/").ok(), Some(("https://hub.example.com".into(), None)));
    assert_eq!(
      parse("HTTPS://abc123@hub.example.com:8443/fox/").ok(),
      Some(("https://hub.example.com:8443/fox".into(), Some("abc123".into())))
    );
    assert_eq!(parse("http://alice:t0k@127.0.0.1:7474").ok().and_then(|p| p.1), Some("t0k".into()));
    assert!(is_remote(Path::new("https://x")));
    assert!(!is_remote(Path::new("/home/me/hub")));
    assert_eq!(url_of(Path::new("http://tok@localhost:7474/")), Some("http://localhost:7474".into()));
    assert_eq!(host_port("https://h.example").ok(), Some(("h.example".into(), 443, true)));
    assert_eq!(host_port("http://[::1]:7474/x").ok(), Some(("[::1]".into(), 7474, false)));
    assert!(is_loopback("[::1]") && is_loopback("127.0.0.1") && is_loopback("LOCALHOST") && !is_loopback("example.com"));
  }
}
