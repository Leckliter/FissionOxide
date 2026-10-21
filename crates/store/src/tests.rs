use super::*;
use serde_json::json;

#[test]
fn tests_never_get_the_users_hub() {
  assert!(is_test_process(), "Cargo runs tests from target/<profile>/deps");
  let root = Store::default_root();
  assert!(!is_user_hub(&root), "the default hub in a test is a scratch one, not {}", root.display());
  let a = Store::scratch_root();
  assert!(a.starts_with(test_hubs_dir()) && a != Store::scratch_root(), "fresh scratch hubs: {}", a.display());
}

#[test]
#[should_panic(expected = "the user's real data hub")]
fn opening_the_users_hub_in_a_test_panics() {
  // refused before anything is read or written
  let _ = Store::open(&platform_dir("FissionOxide", "fissionoxide"));
}

fn temp_root(name: &str) -> PathBuf {
  let d = std::env::temp_dir().join(format!("fox-store-test-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&d);
  d
}

fn design() -> NewItem<'static> {
  NewItem { project: "", folder: None, name: "", kind: ItemKind::Design, ext: "fox" }
}

#[test]
fn projects_folders_and_versions() {
  let root = temp_root("basic");
  let mut s = Store::open(&root).unwrap();
  let p = s.create_project("Brackets").unwrap();
  assert!(s.create_project("brackets").is_err(), "names are unique per hub");
  let f = s.create_folder(&p, None, "Rev A").unwrap();
  let sub = s.create_folder(&p, Some(&f), "Drafts").unwrap();
  assert_eq!(s.folder_path(Some(&sub)).iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Rev A", "Drafts"]);

  let at = NewItem { project: &p, folder: Some(&f), name: "L-bracket", ..design() };
  let meta =
    VersionMeta { description: "first cut".into(), thumbnail_png: Some(vec![1, 2, 3]), ..VersionMeta::default() };
  let (item, v1) = s.create_item(at, b"{\"v\":1}", Origin::Saved, meta).unwrap();
  assert_eq!(v1, 1);
  let v2 = s.add_version(&item, b"{\"v\":2}", Origin::Saved, VersionMeta::default()).unwrap();
  let v3 = s.add_version(&item, b"{\"v\":3}", Origin::Saved, VersionMeta::default()).unwrap();
  assert_eq!((v2, v3), (2, 3));
  assert_eq!(s.read_version(&item, None).unwrap(), b"{\"v\":3}");
  assert_eq!(s.read_version(&item, Some(1)).unwrap(), b"{\"v\":1}");
  assert_eq!(s.thumbnail(&item, Some(1)).unwrap(), [1, 2, 3]);
  assert!(s.thumbnail(&item, Some(2)).is_none());

  // promote v1: it becomes v4 with v1's content
  let v4 = s.promote(&item, 1).unwrap();
  assert_eq!(v4, 4);
  assert_eq!(s.read_version(&item, None).unwrap(), b"{\"v\":1}");
  assert_eq!(s.item(&item).unwrap().latest().unwrap().origin, Origin::Promoted { from: 1 });

  // milestones are protected; the latest can't be deleted
  s.set_milestone(&item, 2, Some("Sent to shop")).unwrap();
  assert!(s.delete_version(&item, 2).is_err());
  assert!(s.delete_version(&item, 4).is_err());
  s.delete_version(&item, 3).unwrap();
  assert_eq!(s.item(&item).unwrap().versions.iter().map(|v| v.number).collect::<Vec<_>>(), [1, 2, 4]);
  // numbering continues after deletions
  assert_eq!(s.add_version(&item, b"5", Origin::Saved, VersionMeta::default()).unwrap(), 5);

  // persisted: a second handle sees everything
  let s2 = Store::open(&root).unwrap();
  let it = s2.item(&item).unwrap();
  assert_eq!(it.versions.len(), 4);
  assert_eq!(it.version(2).unwrap().milestone.as_deref(), Some("Sent to shop"));
  assert_eq!(it.version(1).unwrap().description, "first cut");
  let l = s2.list(&p, Some(&f)).unwrap();
  assert_eq!(l.items.len(), 1);
  assert_eq!(l.folders.len(), 1);
  assert_eq!(s2.list(&p, None).unwrap().folders.len(), 1);
  assert!(!s2.activity(Some(&p), None, 100).is_empty());
  assert!(s2.activity(None, Some(&item), 100).iter().any(|a| a.text.contains("milestone")));
  assert_eq!(s2.search("shop").len(), 1);
  assert_eq!(s2.search("bracket").len(), 1);
  assert_eq!(s2.recent(10).len(), 1);
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn trash_restore_and_purge() {
  let root = temp_root("trash");
  let mut s = Store::open(&root).unwrap();
  let p = s.create_project("P").unwrap();
  let f = s.create_folder(&p, None, "F").unwrap();
  let at = NewItem { project: &p, folder: Some(&f), name: "Part", ..design() };
  let (item, _) = s.create_item(at, b"x", Origin::Saved, VersionMeta::default()).unwrap();
  assert!(s.delete_folder(&f).is_err(), "folder not empty");
  s.trash_item(&item).unwrap();
  assert!(s.list(&p, Some(&f)).unwrap().items.is_empty());
  assert_eq!(s.trash(None).0.len(), 1);
  assert!(s.add_version(&item, b"y", Origin::Saved, VersionMeta::default()).is_err());
  // the folder can go now; restoring puts the item at the project root
  s.delete_folder(&f).unwrap();
  s.restore_item(&item).unwrap();
  assert_eq!(s.list(&p, None).unwrap().items.len(), 1);
  assert!(s.purge_item(&item).is_err(), "only trashed items can be purged");
  s.trash_item(&item).unwrap();
  let dir = s.item_dir(&item);
  assert!(dir.exists());
  s.purge_item(&item).unwrap();
  assert!(!dir.exists());
  assert!(s.item(&item).is_err());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn uploads_copies_and_moves() {
  let root = temp_root("upload");
  let mut s = Store::open(&root).unwrap();
  let p = s.default_project().unwrap();
  assert_eq!(s.project(&p).unwrap().name, "Default Project");
  assert_eq!(s.default_project().unwrap(), p, "reused");
  let q = s.create_project("Other").unwrap();
  let at = NewItem { project: &p, folder: None, name: "Bracket", ..design() };
  let meta =
    VersionMeta { original: Some(("bracket.step".into(), b"ISO-10303-21;".to_vec())), ..VersionMeta::default() };
  let (item, _) = s.create_item(at, b"{}", Origin::Uploaded { file: "bracket.step".into() }, meta).unwrap();
  let v = s.item(&item).unwrap().latest().unwrap().clone();
  let orig = v.original.clone().unwrap();
  assert!(orig.starts_with("v1-") && orig.ends_with(".orig.step"), "{orig}");
  assert!(s.item_dir(&item).join(&orig).exists());
  let raw = NewItem { project: &p, folder: None, name: "Fusion part", kind: ItemKind::File, ext: "f3d" };
  let (f3d, _) =
    s.create_item(raw, b"PK", Origin::Uploaded { file: "part.f3d".into() }, VersionMeta::default()).unwrap();
  let file = s.version_path(&f3d, None).unwrap();
  assert!(file.extension().unwrap() == "f3d" && file.file_name().unwrap().to_string_lossy().starts_with("v1-"));
  let at = NewItem { project: &q, folder: None, name: "Bracket copy", ..design() };
  let (copy, _) = s.copy_item(&item, None, at).unwrap();
  assert_eq!(s.item(&copy).unwrap().project, q);
  s.move_item(&f3d, &q, None).unwrap();
  assert_eq!(s.list(&q, None).unwrap().items.len(), 2);
  assert_eq!(s.find_project("other").unwrap().id, q);
  assert!(
    s.create_item(
      NewItem { project: &q, folder: None, name: "x", kind: ItemKind::File, ext: "../e" },
      b"",
      Origin::Saved,
      VersionMeta::default()
    )
    .is_err()
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn version_files_are_immutable() {
  let root = temp_root("immutable");
  let mut s = Store::open(&root).unwrap();
  let p = s.create_project("P").unwrap();
  let (item, _) = s
    .create_item(
      NewItem { project: &p, folder: None, name: "A", ..design() },
      b"1",
      Origin::Saved,
      VersionMeta::default(),
    )
    .unwrap();
  // a stray file where v2 would go (an interrupted save): it is kept, and the save skips to v3
  std::fs::write(s.item_dir(&item).join("v2.fox"), b"stray").unwrap();
  assert_eq!(s.add_version(&item, b"2", Origin::Saved, VersionMeta::default()).unwrap(), 3);
  assert_eq!(std::fs::read(s.item_dir(&item).join("v2.fox")).unwrap(), b"stray");
  assert_eq!(s.read_version(&item, None).unwrap(), b"2");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn time_formatting() {
  assert_eq!(format_time(0), "1970-01-01 00:00");
  assert_eq!(format_time(1_792_368_000), "2026-10-19 00:00");
  assert_eq!(relative_time(100, 130), "just now");
  assert_eq!(relative_time(0, 7200), "2 h ago");
}

#[test]
fn compare_two_designs() {
  let a = json!({"name": "A", "parameters": [{"name": "w", "expr": "40"}, {"name": "old", "expr": "1"}],
    "features": [{"id": 1, "name": "Sketch1", "kind": {"type": "sketch"}}, {"id": 2, "name": "Extrude1", "kind": {"d": "10"}}]});
  let b = json!({"name": "A", "parameters": [{"name": "w", "expr": "50"}, {"name": "h", "expr": "2"}],
    "features": [{"id": 1, "name": "Base", "kind": {"type": "sketch"}}, {"id": 2, "name": "Extrude1", "kind": {"d": "12"}}, {"id": 3, "name": "Fillet1", "kind": {}}]});
  let c = compare_designs(&a, &b);
  let has = |kind: &str, what: &str, name: &str| c.iter().any(|x| x.kind == kind && x.what == what && x.name == name);
  assert!(has("renamed", "feature", "Base"), "{c:?}");
  assert!(has("changed", "feature", "Extrude1"), "{c:?}");
  assert!(!has("changed", "feature", "Base"), "a rename alone is not an edit: {c:?}");
  assert!(has("added", "feature", "Fillet1"));
  assert!(has("changed", "parameter", "w"));
  assert!(has("added", "parameter", "h"));
  assert!(has("removed", "parameter", "old"));
  assert!(compare_designs(&a, &a).is_empty());
}

fn copy_dir(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for e in std::fs::read_dir(from).unwrap() {
    let e = e.unwrap();
    let t = to.join(e.file_name());
    if e.file_type().unwrap().is_dir() {
      copy_dir(&e.path(), &t)
    } else {
      std::fs::copy(e.path(), t).map(|_| ()).unwrap()
    }
  }
}

#[test]
fn two_writers_merge_instead_of_overwriting() {
  let root = temp_root("merge");
  let mut a = Store::open(&root).unwrap();
  let p = a.create_project("Shared").unwrap();
  let (item, _) = a
    .create_item(NewItem { project: &p, name: "Hinge", ..design() }, b"1", Origin::Saved, VersionMeta::default())
    .unwrap();
  // the CLI (or another machine) opens the hub now, before `a` changes it again
  let mut b = Store::open(&root).unwrap();
  a.create_folder(&p, None, "From A").unwrap();
  let va = a.add_version(&item, b"2a", Origin::Saved, VersionMeta::default()).unwrap();
  // b still has the old index in memory: its writes merge with a's instead of replacing them
  b.create_folder(&p, None, "From B").unwrap();
  let vb = b.add_version(&item, b"2b", Origin::Saved, VersionMeta::default()).unwrap();
  b.rename_item(&item, "Hinge (B)").unwrap();
  assert_eq!((va, vb), (2, 3), "b's save follows a's");
  // two saves that really race (both wrote v2 before either index write) are renumbered
  let mut c = Store::open(&root).unwrap();
  c.write_version(&item, b"4c", Origin::Saved, VersionMeta::default()).unwrap();
  let vd = b.add_version(&item, b"4d", Origin::Saved, VersionMeta::default()).unwrap();
  c.persist().unwrap();
  assert_eq!(vd, 4);

  let s = Store::open(&root).unwrap();
  let names: Vec<_> = s.list(&p, None).unwrap().folders.iter().map(|f| f.name.clone()).collect();
  assert!(names.contains(&"From A".to_string()) && names.contains(&"From B".to_string()), "{names:?}");
  let it = s.item(&item).unwrap();
  assert_eq!(it.name, "Hinge (B)");
  assert_eq!(it.versions.iter().map(|v| v.number).collect::<Vec<_>>(), [1, 2, 3, 4, 5], "the clash is renumbered");
  let contents: Vec<Vec<u8>> = (1..=5).map(|n| s.read_version(&item, Some(n)).unwrap()).collect();
  assert_eq!(contents, [&b"1"[..], b"2a", b"2b", b"4d", b"4c"], "no save lost; the later index write comes after");
  // both activity feeds survive
  assert!(s.activity(None, None, 100).iter().any(|a| a.text.contains("renamed Hinge")));
  assert!(s.activity(None, None, 100).iter().filter(|a| a.text.starts_with("saved Hinge")).count() >= 2);

  // a deletion on one side is kept when the other side didn't touch the record
  let mut c = Store::open(&root).unwrap();
  let mut d = Store::open(&root).unwrap();
  let f = c.list(&p, None).unwrap().folders.iter().find(|f| f.name == "From A").unwrap().id.clone();
  c.delete_folder(&f).unwrap();
  d.create_project("Elsewhere").unwrap();
  let s = Store::open(&root).unwrap();
  assert!(s.folder(&f).is_err(), "the deleted folder stays deleted");
  assert!(s.find_project("Elsewhere").is_ok());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_held_lock_writes_a_conflict_copy_merged_on_the_next_open() {
  let root = temp_root("lock");
  let mut s = Store::open(&root).unwrap();
  let p = s.create_project("P").unwrap();
  // another machine's lock arrives through the sync folder and stays
  std::fs::write(root.join(sync::LOCK), "other-host pid 1").unwrap();
  let before = std::fs::read_to_string(root.join("hub.json")).unwrap();
  let q = s.create_project("Written while locked").unwrap();
  assert_eq!(std::fs::read_to_string(root.join("hub.json")).unwrap(), before, "hub.json untouched");
  assert_eq!(sync::conflict_copies(&root).len(), 1, "the change went to a conflict copy");
  assert!(s.project(&q).is_ok(), "and is still visible to its writer");

  // the lock goes away; the next open merges the copy and removes it
  std::fs::remove_file(root.join(sync::LOCK)).unwrap();
  let s2 = Store::open(&root).unwrap();
  assert_eq!(s2.merged_conflicts(), 1);
  assert!(s2.project(&p).is_ok() && s2.project(&q).is_ok());
  assert!(sync::conflict_copies(&root).is_empty());
  assert!(!root.join(sync::LOCK).exists(), "the lock is released after the merge");

  // a stale lock (a crashed writer) is broken instead of waited on
  let lock = root.join(sync::LOCK);
  std::fs::write(&lock, "crashed").unwrap();
  let old = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
  std::fs::File::options().write(true).open(&lock).unwrap().set_modified(old).unwrap();
  let mut s3 = Store::open(&root).unwrap();
  let t = std::time::Instant::now();
  s3.create_project("After crash").unwrap();
  assert!(t.elapsed() < std::time::Duration::from_secs(1));
  assert!(sync::conflict_copies(&root).is_empty() && !lock.exists());
  assert!(Store::open(&root).unwrap().find_project("After crash").is_ok());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sync_tool_conflict_copies_are_merged() {
  // two machines share a hub through a sync folder and both save while offline
  let root = temp_root("synced");
  let mut a = Store::open(&root).unwrap();
  let p = a.create_project("Team").unwrap();
  let (item, _) = a
    .create_item(NewItem { project: &p, name: "Bracket", ..design() }, b"v1", Origin::Saved, VersionMeta::default())
    .unwrap();
  let other = temp_root("synced-other");
  copy_dir(&root, &other);
  let mut b = Store::open(&other).unwrap();
  b.add_version(&item, b"bob", Origin::Saved, VersionMeta::default()).unwrap();
  b.add_comment(&item, 1, "check the hole spacing").unwrap();
  a.add_version(&item, b"alice", Origin::Saved, VersionMeta::default()).unwrap();
  // the sync tool brings Bob's version file over and keeps his index as a conflicted copy
  copy_dir(&other.join("items"), &root.join("items"));
  std::fs::copy(other.join("hub.json"), root.join("hub (Bob's conflicted copy 2026-10-19).json")).unwrap();
  std::fs::write(root.join("hub.sync-conflict-20261019-xyz.json"), "{ not json").unwrap();

  let s = Store::open(&root).unwrap();
  assert_eq!(s.merged_conflicts(), 1, "the unreadable copy is left for later");
  let it = s.item(&item).unwrap();
  assert_eq!(it.versions.iter().map(|v| v.number).collect::<Vec<_>>(), [1, 2, 3]);
  let all: Vec<Vec<u8>> = it.versions.iter().map(|v| s.read_version(&item, Some(v.number)).unwrap()).collect();
  assert!(all.contains(&b"bob".to_vec()) && all.contains(&b"alice".to_vec()));
  assert_eq!(s.comments(&item, Some(1)).unwrap().len(), 1, "Bob's comment arrived");
  assert_eq!(sync::conflict_copies(&root).len(), 1);
  assert!(root.join("hub.sync-conflict-20261019-xyz.json").exists());
  let _ = std::fs::remove_dir_all(&root);
  let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn comments_on_versions() {
  let root = temp_root("comments");
  let mut s = Store::open(&root).unwrap();
  let p = s.create_project("P").unwrap();
  let (item, _) = s
    .create_item(NewItem { project: &p, name: "Lid", ..design() }, b"1", Origin::Saved, VersionMeta::default())
    .unwrap();
  s.add_version(&item, b"2", Origin::Saved, VersionMeta::default()).unwrap();
  let c1 = s.add_comment(&item, 1, "  Wall too thin here  ").unwrap();
  let c2 = s.add_comment(&item, 2, "Fixed in v2, please check the snap fit").unwrap();
  assert!(s.add_comment(&item, 2, "   ").is_err(), "empty comments are refused");
  assert!(s.add_comment(&item, 9, "nope").is_err());
  assert_ne!(c1, c2);

  let s = Store::open(&root).unwrap();
  let all = s.comments(&item, None).unwrap();
  assert_eq!(
    all.iter().map(|(n, c)| (*n, c.text.as_str())).collect::<Vec<_>>(),
    [(1, "Wall too thin here"), (2, "Fixed in v2, please check the snap fit")]
  );
  assert_eq!(s.comments(&item, Some(2)).unwrap().len(), 1);
  assert!(!all[0].1.author.is_empty());
  assert_eq!(s.search("snap fit").len(), 1, "comments are searchable");
  assert!(s.activity(None, Some(&item), 10).iter().any(|a| a.text.contains("commented on Lid v2")));

  let mut s = s;
  s.delete_comment(&item, &c1).unwrap();
  assert!(s.delete_comment(&item, &c1).is_err());
  assert_eq!(Store::open(&root).unwrap().comments(&item, None).unwrap().len(), 1);
  // promoting a version doesn't copy its comments
  let v3 = s.promote(&item, 2).unwrap();
  assert!(s.comments(&item, Some(v3)).unwrap().is_empty());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn edit_locks_keep_others_from_saving() {
  let root = temp_root("locks");
  let mut alice = Store::open(&root).unwrap();
  alice.set_actor("alice", "s-alice", "a-pc");
  let p = alice.create_project("Locks").unwrap();
  let (item, _) =
    alice.create_item(NewItem { project: &p, name: "Hinge", ..design() }, b"1", Origin::Saved, Default::default()).unwrap();
  let l = alice.take_lock(&item, false).unwrap();
  assert_eq!((l.user.as_str(), l.host.as_str()), ("alice", "a-pc"));
  // taking it again keeps when it was first taken
  assert_eq!(alice.take_lock(&item, false).unwrap().since, l.since);
  alice.add_version(&item, b"2", Origin::Saved, Default::default()).unwrap();

  // another process, another user (as through a sync folder)
  let mut bob = Store::open(&root).unwrap();
  bob.set_actor("bob", "s-bob", "b-pc");
  assert_eq!(bob.lock_of(&item).unwrap().map(|l| l.user.as_str()), Some("alice"));
  let e = bob.add_version(&item, b"3", Origin::Saved, Default::default()).unwrap_err();
  assert!(matches!(e, StoreError::Locked(_)) && e.to_string().contains("locked by alice on a-pc"), "{e}");
  assert!(matches!(bob.take_lock(&item, false), Err(StoreError::Locked(_))));
  assert!(matches!(bob.release_lock(&item), Err(StoreError::Locked(_))));
  assert!(matches!(bob.promote(&item, 1), Err(StoreError::Locked(_))));
  assert!(matches!(bob.trash_item(&item), Err(StoreError::Locked(_))));
  // comments are fine while it's locked
  bob.add_comment(&item, 2, "looks good").unwrap();

  // stealing takes it over, and says so in the activity
  bob.take_lock(&item, true).unwrap();
  assert_eq!(bob.activity(None, Some(&item), 1)[0].text, "took the lock on Hinge from alice");
  assert_eq!(bob.add_version(&item, b"3", Origin::Saved, Default::default()).unwrap(), 3);
  alice.refresh().unwrap();
  assert!(matches!(alice.add_version(&item, b"4", Origin::Saved, Default::default()), Err(StoreError::Locked(_))));
  assert_eq!(bob.my_locks().len(), 1);
  bob.release_lock(&item).unwrap();
  bob.release_lock(&item).unwrap();
  alice.refresh().unwrap();
  assert!(alice.lock_of(&item).unwrap().is_none());
  assert_eq!(alice.add_version(&item, b"4", Origin::Saved, Default::default()).unwrap(), 4);
  assert_eq!(alice.item(&item).unwrap().latest().unwrap().author, "alice");
  assert_eq!(alice.item(&item).unwrap().version(3).unwrap().author, "bob");

  // files can't be locked
  let at = NewItem { project: &p, name: "notes", kind: ItemKind::File, ext: "txt", folder: None };
  let (f, _) = alice.create_item(at, b"x", Origin::Saved, Default::default()).unwrap();
  assert!(alice.take_lock(&f, false).is_err());
}

#[test]
fn presence_files_on_a_local_hub() {
  let root = temp_root("presence");
  let mut a = Store::open(&root).unwrap();
  a.set_actor("alice", "s-a", "a-pc");
  let mut b = Store::open(&root).unwrap();
  b.set_actor("bob", "s-b", "b-pc");
  a.here(&["i1".into(), "i2".into()]).unwrap();
  b.here(&["i1".into()]).unwrap();
  let on_i1: Vec<String> = b.presence(Some("i1")).unwrap().into_iter().map(|p| p.user).collect();
  assert_eq!(on_i1.len(), 2);
  assert!(on_i1.contains(&"alice".to_string()) && on_i1.contains(&"bob".to_string()));
  assert_eq!(b.presence(None).unwrap().len(), 3);
  // closing one keeps when the other was opened
  let since = a.presence(Some("i2")).unwrap()[0].since;
  a.here(&["i2".into()]).unwrap();
  assert_eq!(a.presence(Some("i2")).unwrap()[0].since, since);
  assert_eq!(a.presence(Some("i1")).unwrap().len(), 1);
  a.here(&[]).unwrap();
  assert!(b.presence(Some("i2")).unwrap().is_empty());
  // presence not renewed for a while has ended
  let path = root.join("presence").join("s-b.json");
  let mut h: presence::Here = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  h.time -= presence::TTL + 1;
  std::fs::write(&path, serde_json::to_string(&h).unwrap()).unwrap();
  assert!(a.presence(None).unwrap().is_empty());
}

#[test]
fn ops_round_trip_through_json() {
  let root = temp_root("ops");
  let mut s = Store::open(&root).unwrap();
  let p: String = serde_json::from_value(s.apply(ops::Op::CreateProject { name: "Ops".into() }).unwrap()).unwrap();
  let op = ops::Op::CreateItem {
    project: p.clone(),
    folder: None,
    name: "Cup".into(),
    kind: ItemKind::Design,
    ext: "fox".into(),
    content: ops::Bytes(b"{\"cup\":1}".to_vec()),
    origin: Origin::Saved,
    meta: VersionMeta { thumbnail_png: Some(vec![9, 8, 7]), description: "first".into(), ..Default::default() }.into(),
  };
  // what goes over the wire
  let wire = serde_json::to_string(&op).unwrap();
  assert!(wire.contains("\"op\":\"create_item\"") && wire.contains("\"content\":\"eyJjdXAiOjF9\""), "{wire}");
  let back: ops::Op = serde_json::from_str(&wire).unwrap();
  assert_eq!(back, op);
  let (item, v): (String, u32) = serde_json::from_value(s.apply(back).unwrap()).unwrap();
  assert_eq!(v, 1);
  assert_eq!(s.read_version(&item, None).unwrap(), b"{\"cup\":1}");
  assert_eq!(s.thumbnail(&item, None).unwrap(), [9, 8, 7]);
  let file = s.item(&item).unwrap().latest().unwrap().file.clone();
  assert_eq!(s.item_file(&item, &file).unwrap(), b"{\"cup\":1}");
  assert!(s.item_file(&item, "../hub.json").is_err(), "only files the index lists are served");
  let lock = s.apply(ops::Op::TakeLock { item: item.clone(), steal: false }).unwrap();
  assert_eq!(lock["user"], json!(user()));
}
