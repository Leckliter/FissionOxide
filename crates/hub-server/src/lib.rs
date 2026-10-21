//! The FissionOxide hub server: one data hub (projects, folders, designs and their versions,
//! comments, activity, trash and edit locks; see [`fox_store`]) shared by everyone who has a
//! token, over HTTP and a WebSocket.
//!
//! The hub itself is an ordinary hub directory, so the server can be pointed at an existing
//! one, and a sync folder or the CLI writing to the same directory keeps working: the server
//! re-reads it every couple of seconds and tells its clients when it changed. The app and the
//! CLI reach it through `fox_store`'s remote backend (`--hub https://…`); the protocol is in
//! [`fox_store::remote`]. Besides it, a few read-only JSON resources are there for scripts:
//!
//! ```text
//! GET /api/projects                 the projects
//! GET /api/projects/ID?folder=F     a project folder's folders and items
//! GET /api/items/ID                 an item with its versions and comments
//! GET /api/activity?project=&item=&limit=
//! GET /api/trash
//! GET /api/search?q=TEXT
//! ```
//!
//! Every request needs `Authorization: Bearer TOKEN` (WebSocket clients may pass `?token=`),
//! with a token from [`Config::tokens`]; what a request does is recorded as the token's user.
//! The server speaks plain HTTP: put it behind a TLS-terminating proxy (Caddy, nginx) to serve
//! `https://` and `wss://`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use fox_store::ops::Op;
use fox_store::presence::{Here, Presence};
use fox_store::{Store, StoreError};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How often the hub directory is re-read for changes made beside the server.
const RESCAN: Duration = Duration::from_secs(2);

/// What to serve, and to whom.
#[derive(Debug, Clone, Default)]
pub struct Config {
  /// The hub directory.
  pub root: PathBuf,
  /// Access token → the user it belongs to.
  pub tokens: HashMap<String, String>,
  /// Serve without tokens (a trusted network only): a request without a known token acts as
  /// `guest`, or as the user its `X-Fox-User` header names.
  pub open: bool,
}

struct Hub {
  store: Mutex<Store>,
  /// The index revision: bumped on every change, starting from the time the server started so
  /// a client's cache from an earlier run never looks current.
  rev: AtomicU64,
  cfg: Config,
  presence: Mutex<HashMap<String, Entry>>,
  events: broadcast::Sender<String>,
  next_socket: AtomicU64,
}

/// One session's presence: kept while its WebSocket is open, or until a heartbeat expires.
struct Entry {
  here: Here,
  socket: Option<u64>,
  expires: Option<Instant>,
}

type Shared = Arc<Hub>;

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
  m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
  fn bump(&self) -> u64 {
    let rev = self.rev.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = self.events.send(json!({"type": "changed", "rev": rev}).to_string());
    rev
  }

  fn presence_list(&self) -> Vec<Presence> {
    let now = Instant::now();
    let p = guard(&self.presence);
    p.values()
      .filter(|e| e.expires.is_none_or(|t| t > now))
      .flat_map(|e| e.here.entries().collect::<Vec<_>>())
      .collect()
  }

  fn presence_changed(&self) {
    let _ = self.events.send(json!({"type": "presence", "presence": self.presence_list()}).to_string());
  }

  /// Drops heartbeats that ran out; whether any did.
  fn prune(&self) -> bool {
    let now = Instant::now();
    let mut p = guard(&self.presence);
    let n = p.len();
    p.retain(|_, e| e.expires.is_none_or(|t| t > now));
    p.len() != n
  }

  /// Picks up changes made to the hub directory beside the server.
  fn rescan(&self) {
    let changed = guard(&self.store).reload();
    match changed {
      Ok(true) => {
        self.bump();
      }
      Ok(false) => {}
      Err(e) => log::warn!("couldn't re-read the hub: {e}"),
    }
  }

  /// The user a request acts as.
  fn user(&self, headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    let bearer = headers
      .get(header::AUTHORIZATION)
      .and_then(|v| v.to_str().ok())
      .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
      .map(str::trim);
    let token = bearer.or(query_token);
    if let Some(u) = token.and_then(|t| self.cfg.tokens.get(t)) {
      return Some(u.clone());
    }
    self.cfg.open.then(|| {
      headers
        .get("x-fox-user")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or("guest")
        .to_string()
    })
  }
}

/// An error answer: `{error, kind}` with a fitting status.
struct Failure(StatusCode, String, &'static str);

impl From<StoreError> for Failure {
  fn from(e: StoreError) -> Self {
    let (status, kind) = match &e {
      StoreError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
      StoreError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid"),
      StoreError::Locked(_) => (StatusCode::CONFLICT, "locked"),
      _ => (StatusCode::INTERNAL_SERVER_ERROR, "server"),
    };
    Failure(status, e.to_string(), kind)
  }
}

impl IntoResponse for Failure {
  fn into_response(self) -> Response {
    (self.0, Json(json!({"error": self.1, "kind": self.2}))).into_response()
  }
}

fn unauthorized() -> Failure {
  Failure(StatusCode::UNAUTHORIZED, "a valid access token is needed (Authorization: Bearer TOKEN)".into(), "auth")
}

type Answer = std::result::Result<Response, Failure>;

/// Runs `f` on the store off the async threads.
async fn with_store<T: Send + 'static>(
  hub: &Shared,
  f: impl FnOnce(&Hub, &mut Store) -> Result<T, StoreError> + Send + 'static,
) -> Result<T, Failure> {
  let hub = hub.clone();
  tokio::task::spawn_blocking(move || {
    let mut st = guard(&hub.store);
    f(&hub, &mut st)
  })
  .await
  .map_err(|e| Failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), "server"))?
  .map_err(Failure::from)
}

#[derive(Deserialize)]
struct TokenQuery {
  token: Option<String>,
}

async fn root_page() -> impl IntoResponse {
  format!(
    "FissionOxide hub server {VERSION}\nConnect with: fissionoxide-cli --hub <this URL> --token TOKEN hub projects\n"
  )
}

async fn whoami(State(hub): State<Shared>, headers: HeaderMap) -> Answer {
  let user = hub.user(&headers, None).ok_or_else(unauthorized)?;
  Ok(Json(json!({"user": user, "server": format!("fox-hub-server {VERSION}")})).into_response())
}

#[derive(Deserialize)]
struct IndexQuery {
  rev: Option<u64>,
}

async fn index(State(hub): State<Shared>, headers: HeaderMap, Query(q): Query<IndexQuery>) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, move |hub, st| {
    if st.reload()? {
      hub.bump();
    }
    let rev = hub.rev.load(Ordering::SeqCst);
    Ok(if q.rev == Some(rev) {
      json!({"rev": rev, "unchanged": true})
    } else {
      json!({"rev": rev, "index": st.snapshot()})
    })
  })
  .await?;
  Ok(Json(v).into_response())
}

#[derive(Deserialize)]
struct OpRequest {
  #[serde(default)]
  session: String,
  #[serde(default)]
  host: String,
  op: Op,
}

async fn op(State(hub): State<Shared>, headers: HeaderMap, Json(req): Json<OpRequest>) -> Answer {
  let user = hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, move |hub, st| {
    st.reload()?;
    let session = if req.session.is_empty() { format!("http-{user}") } else { req.session };
    st.set_actor(&user, &session, &req.host);
    let result = st.apply(req.op)?;
    Ok(json!({"result": result, "rev": hub.bump()}))
  })
  .await?;
  Ok(Json(v).into_response())
}

async fn file(State(hub): State<Shared>, headers: HeaderMap, Path((item, name)): Path<(String, String)>) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let bytes = with_store(&hub, move |_, st| st.item_file(&item, &name)).await?;
  Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

async fn presence_get(State(hub): State<Shared>, headers: HeaderMap) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  Ok(Json(hub.presence_list()).into_response())
}

async fn presence_post(State(hub): State<Shared>, headers: HeaderMap, Json(mut here): Json<Here>) -> Answer {
  let user = hub.user(&headers, None).ok_or_else(unauthorized)?;
  here.user = user;
  if here.session.is_empty() {
    return Err(Failure(StatusCode::BAD_REQUEST, "missing 'session'".into(), "invalid"));
  }
  {
    let mut p = guard(&hub.presence);
    if here.items.is_empty() {
      p.remove(&here.session);
    } else {
      let ttl = Duration::from_secs(fox_store::presence::TTL);
      p.insert(here.session.clone(), Entry { here, socket: None, expires: Some(Instant::now() + ttl) });
    }
  }
  hub.presence_changed();
  Ok(Json(json!({})).into_response())
}

async fn ws(
  State(hub): State<Shared>,
  headers: HeaderMap,
  Query(q): Query<TokenQuery>,
  upgrade: WebSocketUpgrade,
) -> Answer {
  let user = hub.user(&headers, q.token.as_deref()).ok_or_else(unauthorized)?;
  Ok(upgrade.on_upgrade(move |socket| socket_loop(hub, user, socket)))
}

async fn socket_loop(hub: Shared, user: String, socket: WebSocket) {
  let id = hub.next_socket.fetch_add(1, Ordering::SeqCst);
  let mut events = hub.events.subscribe();
  let (mut tx, mut rx) = socket.split();
  let hello = json!({"type": "hello", "rev": hub.rev.load(Ordering::SeqCst), "presence": hub.presence_list()});
  if tx.send(Message::text(hello.to_string())).await.is_err() {
    return;
  }
  loop {
    tokio::select! {
      msg = rx.next() => match msg {
        Some(Ok(Message::Text(t))) => {
          let Ok(mut here) = serde_json::from_str::<Here>(t.as_str()) else { continue };
          if here.session.is_empty() {
            continue;
          }
          here.user = user.clone();
          {
            let mut p = guard(&hub.presence);
            if here.items.is_empty() {
              p.remove(&here.session);
            } else {
              p.insert(here.session.clone(), Entry { here, socket: Some(id), expires: None });
            }
          }
          hub.presence_changed();
        }
        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
        Some(Ok(_)) => {}
      },
      ev = events.recv() => match ev {
        Ok(text) => {
          if tx.send(Message::text(text)).await.is_err() {
            break;
          }
        }
        Err(broadcast::error::RecvError::Lagged(_)) => {
          // missed some: say where things stand now
          let now = json!({"type": "hello", "rev": hub.rev.load(Ordering::SeqCst), "presence": hub.presence_list()});
          if tx.send(Message::text(now.to_string())).await.is_err() {
            break;
          }
        }
        Err(broadcast::error::RecvError::Closed) => break,
      },
    }
  }
  // its sessions have closed their designs
  let gone = {
    let mut p = guard(&hub.presence);
    let n = p.len();
    p.retain(|_, e| e.socket != Some(id));
    p.len() != n
  };
  if gone {
    hub.presence_changed();
  }
}

#[derive(Deserialize)]
struct ListQuery {
  folder: Option<String>,
}

async fn projects(State(hub): State<Shared>, headers: HeaderMap) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, |_, st| Ok(json!(st.projects()))).await?;
  Ok(Json(v).into_response())
}

async fn project(
  State(hub): State<Shared>,
  headers: HeaderMap,
  Path(id): Path<String>,
  Query(q): Query<ListQuery>,
) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, move |_, st| {
    let l = st.list(&id, q.folder.as_deref())?;
    Ok(json!({"project": st.project(&id)?, "folders": l.folders, "items": l.items}))
  })
  .await?;
  Ok(Json(v).into_response())
}

async fn item(State(hub): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, move |hub, st| {
    let presence: Vec<Presence> = hub.presence_list().into_iter().filter(|p| p.item == id).collect();
    Ok(json!({"item": st.item(&id)?, "presence": presence}))
  })
  .await?;
  Ok(Json(v).into_response())
}

#[derive(Deserialize)]
struct ActivityQuery {
  project: Option<String>,
  item: Option<String>,
  limit: Option<usize>,
}

async fn activity(State(hub): State<Shared>, headers: HeaderMap, Query(q): Query<ActivityQuery>) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, move |_, st| {
    Ok(json!(st.activity(q.project.as_deref(), q.item.as_deref(), q.limit.unwrap_or(100))))
  })
  .await?;
  Ok(Json(v).into_response())
}

async fn trash(State(hub): State<Shared>, headers: HeaderMap) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, |_, st| {
    let (items, projects) = st.trash(None);
    Ok(json!({"items": items, "projects": projects}))
  })
  .await?;
  Ok(Json(v).into_response())
}

#[derive(Deserialize)]
struct SearchQuery {
  q: String,
}

async fn search(State(hub): State<Shared>, headers: HeaderMap, Query(q): Query<SearchQuery>) -> Answer {
  hub.user(&headers, None).ok_or_else(unauthorized)?;
  let v = with_store(&hub, move |_, st| Ok(json!(st.search(&q.q)))).await?;
  Ok(Json(v).into_response())
}

fn router(hub: Shared) -> Router {
  Router::new()
    .route("/", get(root_page))
    .route("/api/whoami", get(whoami))
    .route("/api/index", get(index))
    .route("/api/op", axum::routing::post(op))
    .route("/api/files/{item}/{file}", get(file))
    .route("/api/presence", get(presence_get).post(presence_post))
    .route("/api/ws", get(ws))
    .route("/api/projects", get(projects))
    .route("/api/projects/{id}", get(project))
    .route("/api/items/{id}", get(item))
    .route("/api/activity", get(activity))
    .route("/api/trash", get(trash))
    .route("/api/search", get(search))
    .layer(DefaultBodyLimit::max(2 << 30))
    .with_state(hub)
}

/// Serves the hub in `cfg` on `listener` until `shutdown` completes.
pub async fn serve(
  listener: tokio::net::TcpListener,
  cfg: Config,
  shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
  let store = Store::open(&cfg.root).map_err(std::io::Error::other)?;
  let start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
  let hub = Arc::new(Hub {
    store: Mutex::new(store),
    rev: AtomicU64::new(start as u64),
    cfg,
    presence: Mutex::new(HashMap::new()),
    events: broadcast::channel(256).0,
    next_socket: AtomicU64::new(1),
  });
  let bg = hub.clone();
  let scan = tokio::spawn(async move {
    let mut tick = tokio::time::interval(RESCAN);
    loop {
      tick.tick().await;
      if bg.prune() {
        bg.presence_changed();
      }
      let h = bg.clone();
      let _ = tokio::task::spawn_blocking(move || h.rescan()).await;
    }
  });
  let r = axum::serve(listener, router(hub)).with_graceful_shutdown(shutdown).await;
  scan.abort();
  r
}

/// A server running on its own threads (for tests, or to embed one).
pub struct Server {
  pub addr: SocketAddr,
  stop: Option<tokio::sync::oneshot::Sender<()>>,
  thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
  /// Starts serving `cfg` on `addr` (`127.0.0.1:0` picks a free port: see [`Server::url`]).
  pub fn start(addr: &str, cfg: Config) -> std::io::Result<Server> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let listener = rt.block_on(tokio::net::TcpListener::bind(addr))?;
    let addr = listener.local_addr()?;
    // fail here, not on the server's thread, if the hub can't be opened
    Store::open(&cfg.root).map_err(std::io::Error::other)?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let thread = std::thread::Builder::new().name("fox-hub-server".into()).spawn(move || {
      let r = rt.block_on(serve(listener, cfg, async {
        let _ = stopped.await;
      }));
      if let Err(e) = r {
        log::error!("hub server: {e}");
      }
      rt.shutdown_timeout(Duration::from_secs(1));
    })?;
    Ok(Server { addr, stop: Some(stop), thread: Some(thread) })
  }

  pub fn url(&self) -> String {
    format!("http://{}", self.addr)
  }

  /// Stops the server and waits for it.
  pub fn stop(mut self) {
    self.halt();
  }

  fn halt(&mut self) {
    if let Some(s) = self.stop.take() {
      let _ = s.send(());
    }
    if let Some(t) = self.thread.take() {
      let _ = t.join();
    }
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.halt();
  }
}

/// A new random access token: 32 hex digits.
pub fn new_token() -> String {
  use std::hash::{BuildHasher, Hasher};
  let mut out = String::new();
  for i in 0..2u8 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u8(i);
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.write_u32(std::process::id());
    out.push_str(&format!("{:016x}", h.finish()));
  }
  out
}

/// Reads a tokens file: a `user=token` per line (blank lines and `#` comments skipped).
pub fn parse_tokens(text: &str) -> Result<HashMap<String, String>, String> {
  let mut out = HashMap::new();
  for (n, line) in text.lines().enumerate() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let (user, token) = line.split_once('=').ok_or_else(|| format!("line {}: expected user=token", n + 1))?;
    let (user, token) = (user.trim(), token.trim());
    if user.is_empty() || token.len() < 8 {
      return Err(format!("line {}: a user and a token of at least 8 characters are needed", n + 1));
    }
    out.insert(token.to_string(), user.to_string());
  }
  Ok(out)
}
