//! The hub server run in-process, used through `fox_store`'s remote backend as the app and the
//! CLI use it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use fox_hub_server::{Config, Server};
use fox_store::{ItemKind, NewItem, Origin, Store, StoreError, VersionMeta};

const ALICE: &str = "alice-token-0001";
const BOB: &str = "bob-token-000002";

fn server(name: &str) -> (Server, std::path::PathBuf) {
  let root = Store::scratch_root().with_extension(format!("server-{name}"));
  let _ = std::fs::remove_dir_all(&root);
  let tokens = HashMap::from([(ALICE.to_string(), "alice".to_string()), (BOB.to_string(), "bob".to_string())]);
  let s = Server::start("127.0.0.1:0", Config { root: root.clone(), tokens, open: false }).unwrap();
  (s, root)
}

fn design<'a>(project: &'a str, name: &'a str) -> NewItem<'a> {
  NewItem { project, folder: None, name, kind: ItemKind::Design, ext: "fox" }
}

/// Waits up to 10 s for `f` to hold.
fn eventually(what: &str, mut f: impl FnMut() -> bool) {
  let start = Instant::now();
  while !f() {
    assert!(start.elapsed() < Duration::from_secs(10), "timed out waiting for {what}");
    std::thread::sleep(Duration::from_millis(50));
  }
}

#[test]
fn projects_items_versions_comments_activity_and_trash() {
  let (srv, _root) = server("model");
  let url = srv.url();
  let mut a = Store::open_remote(&url, Some(ALICE)).unwrap();
  assert_eq!((a.user(), a.server()), ("alice", Some(url.as_str())));
  let p = a.create_project("Brackets").unwrap();
  assert!(matches!(a.create_project("brackets"), Err(StoreError::Invalid(_))), "the server's checks come back");
  let f = a.create_folder(&p, None, "Rev A").unwrap();
  let meta = VersionMeta { description: "first cut".into(), thumbnail_png: Some(vec![1, 2, 3]), ..Default::default() };
  let at = NewItem { folder: Some(&f), ..design(&p, "L-bracket") };
  let (item, v1) = a.create_item(at, b"{\"v\":1}", Origin::Saved, meta).unwrap();
  assert_eq!(v1, 1);
  assert_eq!(a.add_version(&item, b"{\"v\":2}", Origin::Saved, VersionMeta::default()).unwrap(), 2);
  // the mirror is up to date after each change
  assert_eq!(a.item(&item).unwrap().versions.len(), 2);
  assert_eq!(a.list(&p, Some(&f)).unwrap().items[0].name, "L-bracket");
  // files are fetched, then cached
  assert_eq!(a.read_version(&item, Some(1)).unwrap(), b"{\"v\":1}");
  assert_eq!(a.read_version(&item, None).unwrap(), b"{\"v\":2}");
  assert_eq!(a.thumbnail(&item, Some(1)).unwrap(), [1, 2, 3]);
  assert!(a.version_path(&item, Some(1)).unwrap().starts_with(a.root()));

  // bob sees it all, and what he does is his
  let mut b = Store::open_remote(&url, Some(BOB)).unwrap();
  assert_eq!(b.user(), "bob");
  assert_eq!(b.find_project("Brackets").unwrap().id, p);
  let c = b.add_comment(&item, 2, "check the fillet").unwrap();
  assert_eq!(b.promote(&item, 1).unwrap(), 3);
  a.refresh().unwrap();
  let it = a.item(&item).unwrap();
  assert_eq!(it.version(1).unwrap().author, "alice");
  assert_eq!(it.version(3).unwrap().author, "bob");
  assert_eq!(a.comments(&item, None).unwrap()[0].1.author, "bob");
  assert_eq!(a.read_version(&item, None).unwrap(), b"{\"v\":1}");
  let feed: Vec<(String, String)> =
    a.activity(Some(&p), None, 10).iter().map(|x| (x.author.clone(), x.text.clone())).collect();
  assert_eq!(feed[0], ("bob".into(), "promoted L-bracket v1 to v3".into()));
  assert!(feed.iter().any(|x| x.0 == "alice" && x.1 == "created L-bracket"));
  assert_eq!(a.search("fillet").len(), 1);

  a.set_milestone(&item, 2, Some("Sent to shop")).unwrap();
  a.delete_comment(&item, &c).unwrap();
  a.rename_item(&item, "L bracket").unwrap();
  let (copy, _) = a.copy_item(&item, Some(2), design(&p, "Copy")).unwrap();
  assert_eq!(a.read_version(&copy, None).unwrap(), b"{\"v\":2}");
  a.trash_item(&copy).unwrap();
  assert_eq!(a.trash(None).0.len(), 1);
  a.restore_item(&copy).unwrap();
  a.trash_item(&copy).unwrap();
  a.purge_item(&copy).unwrap();
  assert!(a.item(&copy).is_err());
  a.move_item(&item, &p, None).unwrap();
  a.delete_folder(&f).unwrap();
  b.refresh().unwrap();
  assert_eq!(b.item(&item).unwrap().name, "L bracket");
  assert!(b.item(&copy).is_err());
  assert!(matches!(b.read_version("nope", None), Err(StoreError::NotFound(_))));
  srv.stop();
}

#[test]
fn tokens_are_needed() {
  let (srv, _root) = server("auth");
  let url = srv.url();
  let e = Store::open_remote(&url, Some("not-a-token-at-all")).err().unwrap();
  assert!(e.to_string().contains("refused the access token"), "{e}");
  // a token in the URL works too
  let st = Store::open_remote(&url.replace("http://", &format!("http://{BOB}@")), None).unwrap();
  assert_eq!(st.user(), "bob");
  // and so does one registered for the process (the CLI's --token)
  let other = format!("http://localhost:{}", srv.addr.port());
  fox_store::remote::use_token(&other, ALICE);
  assert_eq!(Store::open(std::path::Path::new(&other)).unwrap().user(), "alice");
}

#[test]
fn locks_are_taken_released_and_stolen() {
  let (srv, _root) = server("locks");
  let url = srv.url();
  let mut a = Store::open_remote(&url, Some(ALICE)).unwrap();
  let mut b = Store::open_remote(&url, Some(BOB)).unwrap();
  let p = a.create_project("Locks").unwrap();
  let (item, _) = a.create_item(design(&p, "Hinge"), b"1", Origin::Saved, Default::default()).unwrap();
  let l = a.take_lock(&item, false).unwrap();
  assert_eq!(l.user, "alice");
  assert!(a.is_mine(a.lock_of(&item).unwrap().unwrap()));

  b.refresh().unwrap();
  let lock = b.lock_of(&item).unwrap().unwrap().clone();
  assert!(!b.is_mine(&lock) && lock.user == "alice");
  let e = b.add_version(&item, b"2", Origin::Saved, Default::default()).unwrap_err();
  assert!(matches!(e, StoreError::Locked(_)) && e.to_string().contains("locked by alice"), "{e}");
  assert!(matches!(b.take_lock(&item, false), Err(StoreError::Locked(_))));
  assert!(matches!(b.release_lock(&item), Err(StoreError::Locked(_))));
  b.take_lock(&item, true).unwrap();
  assert_eq!(b.add_version(&item, b"2", Origin::Saved, Default::default()).unwrap(), 2);
  assert!(matches!(a.add_version(&item, b"3", Origin::Saved, Default::default()), Err(StoreError::Locked(_))));
  assert!(a.activity(None, Some(&item), 5).iter().any(|x| x.text == "took the lock on Hinge from alice"));
  b.release_lock(&item).unwrap();
  assert_eq!(a.add_version(&item, b"3", Origin::Saved, Default::default()).unwrap(), 3);
  assert!(a.lock_of(&item).unwrap().is_none());
}

#[test]
fn presence_is_live_and_ends_with_the_connection() {
  let (srv, _root) = server("presence");
  let url = srv.url();
  let mut a = Store::open_remote(&url, Some(ALICE)).unwrap();
  let b = Store::open_remote(&url, Some(BOB)).unwrap();
  let p = a.create_project("Here").unwrap();
  let (item, _) = a.create_item(design(&p, "Cup"), b"1", Origin::Saved, Default::default()).unwrap();
  eventually("the WebSocket", || fox_store::remote::is_live(std::path::Path::new(&url)));

  // alice opens it: bob sees her at once
  let seen = fox_store::remote::generation();
  a.here(std::slice::from_ref(&item)).unwrap();
  eventually("alice's presence", || b.presence(Some(&item)).unwrap().iter().any(|x| x.user == "alice"));
  assert!(fox_store::remote::generation() > seen, "the UI is told something changed");
  let mine = &a.presence(Some(&item)).unwrap()[0];
  assert_eq!(mine.session, a.session());

  // a client of its own (another app): present while its socket is open
  let (mut ws, _) = {
    use tungstenite::client::IntoClientRequest;
    let mut req = format!("ws://{}/api/ws", srv.addr).into_client_request().unwrap();
    req.headers_mut().insert("Authorization", format!("Bearer {BOB}").parse().unwrap());
    tungstenite::connect(req).unwrap()
  };
  if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_mut() {
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
  }
  let here = serde_json::json!({"type": "here", "user": "mallory", "session": "other-app", "items": [[item, 0]]});
  ws.send(tungstenite::Message::text(here.to_string())).unwrap();
  eventually("bob's other app", || {
    a.presence(Some(&item)).unwrap().iter().any(|x| x.session == "other-app" && x.user == "bob")
  });
  // its socket also hears about changes as they happen
  a.add_comment(&item, 1, "hello").unwrap();
  let mut changed = false;
  while !changed {
    if let tungstenite::Message::Text(t) = ws.read().unwrap() {
      changed = t.as_str().contains("\"changed\"");
    }
  }
  drop(ws);
  eventually("bob's other app to go", || a.presence(Some(&item)).unwrap().iter().all(|x| x.session != "other-app"));

  // closing the design ends alice's presence
  a.here(&[]).unwrap();
  eventually("alice to go", || b.presence(Some(&item)).unwrap().is_empty());
}

#[test]
fn the_hub_directory_still_works_as_a_sync_folder() {
  let (srv, root) = server("sync");
  let url = srv.url();
  let mut remote = Store::open_remote(&url, Some(ALICE)).unwrap();
  let p = remote.create_project("Shared").unwrap();
  // the server's hub is a plain hub directory: the CLI (or a sync tool) can write to it beside
  // the server, and the server's clients see it
  let mut local = Store::open(&root).unwrap();
  assert_eq!(local.find_project("Shared").unwrap().id, p);
  let (item, _) = local.create_item(design(&p, "From disk"), b"disk", Origin::Saved, Default::default()).unwrap();
  eventually("the server to pick up the disk change", || remote.refresh().is_ok() && remote.item(&item).is_ok());
  assert_eq!(remote.read_version(&item, None).unwrap(), b"disk");
  // and what clients do lands in the directory
  remote.add_version(&item, b"net", Origin::Saved, Default::default()).unwrap();
  local.refresh().unwrap();
  assert_eq!(local.read_version(&item, None).unwrap(), b"net");
  assert_eq!(local.item(&item).unwrap().latest().unwrap().author, "alice");
}

fn http(method: &str, url: &str, token: Option<&str>, body: Option<serde_json::Value>) -> (u16, serde_json::Value) {
  let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).proxy(None).build().new_agent();
  let resp = match (method, body) {
    ("POST", Some(b)) => {
      let mut r = agent.post(url).header("Content-Type", "application/json");
      if let Some(t) = token {
        r = r.header("Authorization", format!("Bearer {t}"));
      }
      r.send(b.to_string())
    }
    _ => {
      let mut r = agent.get(url);
      if let Some(t) = token {
        r = r.header("Authorization", format!("Bearer {t}"));
      }
      r.call()
    }
  };
  let mut resp = resp.unwrap();
  let status = resp.status().as_u16();
  let text = resp.body_mut().read_to_string().unwrap();
  (status, serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
}

#[test]
fn json_resources_and_presence_over_plain_http() {
  let (srv, _root) = server("rest");
  let url = srv.url();
  assert_eq!(http("GET", &format!("{url}/api/projects"), None, None).0, 401);
  assert_eq!(http("GET", &format!("{url}/api/projects"), Some("bad-token-xyz"), None).0, 401);
  let mut a = Store::open_remote(&url, Some(ALICE)).unwrap();
  let p = a.create_project("Rest").unwrap();
  let (item, _) = a.create_item(design(&p, "Gear"), b"g", Origin::Saved, Default::default()).unwrap();
  a.add_comment(&item, 1, "mesh it finer").unwrap();

  let (code, v) = http("GET", &format!("{url}/api/projects"), Some(BOB), None);
  assert_eq!((code, v[0]["name"].as_str()), (200, Some("Rest")));
  let (_, v) = http("GET", &format!("{url}/api/projects/{p}"), Some(BOB), None);
  assert_eq!(v["items"][0]["name"], "Gear");
  let (_, v) = http("GET", &format!("{url}/api/items/{item}"), Some(BOB), None);
  assert_eq!(v["item"]["versions"][0]["comments"][0]["text"], "mesh it finer");
  let (_, v) = http("GET", &format!("{url}/api/search?q=finer"), Some(BOB), None);
  assert_eq!(v.as_array().map(Vec::len), Some(1));
  let (_, v) = http("GET", &format!("{url}/api/activity?project={p}&limit=1"), Some(BOB), None);
  assert_eq!(v[0]["author"], "alice");
  assert_eq!(http("GET", &format!("{url}/api/items/nope"), Some(BOB), None).0, 404);
  assert_eq!(http("GET", &format!("{url}/api/files/{item}/..%2Fhub.json"), Some(BOB), None).0, 404);

  // an op the server refuses says why, with a kind the client turns back into the error
  let op = serde_json::json!({"op": {"op": "create_project", "name": "  "}});
  let (code, v) = http("POST", &format!("{url}/api/op"), Some(BOB), Some(op));
  assert_eq!((code, v["kind"].as_str()), (400, Some("invalid")));

  // presence by heartbeat, for clients without a WebSocket; the user is the token's
  let here = serde_json::json!({"user": "mallory", "session": "cli-1", "items": [[item, 5]]});
  assert_eq!(http("POST", &format!("{url}/api/presence"), Some(BOB), Some(here)).0, 200);
  let (_, v) = http("GET", &format!("{url}/api/presence"), Some(ALICE), None);
  assert_eq!((v[0]["user"].as_str(), v[0]["item"].as_str()), (Some("bob"), Some(item.as_str())));
  eventually("bob's presence in alice's store", || a.presence(Some(&item)).unwrap().iter().any(|x| x.user == "bob"));
  let gone = serde_json::json!({"session": "cli-1", "items": []});
  http("POST", &format!("{url}/api/presence"), Some(BOB), Some(gone));
  assert_eq!(http("GET", &format!("{url}/api/presence"), Some(ALICE), None).1, serde_json::json!([]));
}

#[test]
fn tokens_files() {
  let t = fox_hub_server::parse_tokens("# team\nalice = 0123456789abcdef\n\nbob=fedcba9876543210\n").unwrap();
  assert_eq!(t.get("0123456789abcdef").map(String::as_str), Some("alice"));
  assert_eq!(t.len(), 2);
  assert!(fox_hub_server::parse_tokens("alice=short").is_err());
  assert!(fox_hub_server::parse_tokens("no equals sign here").is_err());
  let a = fox_hub_server::new_token();
  assert!(a.len() == 32 && a != fox_hub_server::new_token());
}
