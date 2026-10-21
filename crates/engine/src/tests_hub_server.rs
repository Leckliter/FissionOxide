//! The `hub.*` commands against a hub server run in-process: connecting, saving and opening
//! designs, edit locks and presence, as the app and the CLI use them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::Session;

const ALICE: &str = "alice-engine-token";
const BOB: &str = "bob-engine-token-2";

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

#[test]
fn designs_locks_and_presence_on_a_hub_server() {
  let root = fox_store::Store::scratch_root().with_extension("engine-server");
  let tokens = HashMap::from([(ALICE.to_string(), "alice".to_string()), (BOB.to_string(), "bob".to_string())]);
  let srv = fox_hub_server::Server::start("127.0.0.1:0", fox_hub_server::Config { root, tokens, open: false }).unwrap();
  let url = srv.url();

  let mut a = Session::default();
  assert!(a.execute("hub.connect", &json!({"url": url, "token": "wrong-token"})).is_err());
  let c = run(&mut a, "hub.connect", json!({"url": url, "token": ALICE, "remember": false}));
  assert_eq!(c["user"], "alice");
  assert_eq!(a.hub_root().to_string_lossy(), url);
  let info = run(&mut a, "hub.info", json!({}));
  assert_eq!((info["server"].as_str(), info["user"].as_str()), (Some(url.as_str()), Some("alice")));

  run(&mut a, "box", json!({"width": 20, "depth": 20, "height": 10}));
  let saved = run(&mut a, "hub.save", json!({"name": "Block", "description": "first"}));
  let item = saved["item"].as_str().unwrap().to_string();
  let l = run(&mut a, "hub.lock", json!({}));
  assert_eq!((l["lock"]["user"].as_str(), l["lock"]["mine"].as_bool()), (Some("alice"), Some(true)));
  run(&mut a, "hub.here", json!({}));

  // bob, on another machine
  let mut b =
    Session { store_root: Some(url.replace("http://", &format!("http://{BOB}@")).into()), ..Default::default() };
  run(&mut b, "hub.open", json!({"item": item}));
  assert_eq!(b.design.features.len(), 1, "the design came over the wire");
  let v = run(&mut b, "hub.versions", json!({"item": item}));
  assert_eq!((v["item"]["lock"]["user"].as_str(), v["item"]["lock"]["mine"].as_bool()), (Some("alice"), Some(false)));
  // he sees alice has it open
  let start = Instant::now();
  while !run(&mut b, "hub.presence", json!({"item": item})).as_array().unwrap().iter().any(|p| p["user"] == "alice") {
    assert!(start.elapsed() < Duration::from_secs(10), "alice's presence never arrived");
    std::thread::sleep(Duration::from_millis(50));
  }
  run(
    &mut b,
    "box",
    json!({"plane": {"origin": [0, 0, 10], "normal": [0, 0, 1]}, "width": 5, "depth": 5, "height": 5, "operation": "join"}),
  );
  let e = b.execute("hub.save", &json!({})).unwrap_err();
  assert!(e.0.contains("locked by alice"), "{}", e.0);
  run(&mut b, "hub.lock", json!({"steal": true}));
  assert_eq!(run(&mut b, "hub.save", json!({"description": "taller"}))["version"], 2);

  let e = a.execute("hub.save", &json!({})).unwrap_err();
  assert!(e.0.contains("locked by bob"), "{}", e.0);
  let v = run(&mut a, "hub.versions", json!({}));
  assert_eq!(v["versions"][0]["author"], "bob");
  assert_eq!(v["versions"][1]["author"], "alice");
  let feed = run(&mut a, "hub.activity", json!({"item": item}));
  assert!(feed.as_array().unwrap().iter().any(|x| x["text"] == "took the lock on Block from alice"), "{feed}");
  run(&mut b, "hub.unlock", json!({}));
  assert_eq!(run(&mut a, "hub.save", json!({}))["version"], 3);

  run(&mut a, "hub.disconnect", json!({}));
  assert!(run(&mut a, "hub.info", json!({}))["server"].is_null());
}
