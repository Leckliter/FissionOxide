use super::*;
use serde_json::json;

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
  assert_eq!(v.original.as_deref(), Some("v1.orig.step"));
  assert!(s.item_dir(&item).join("v1.orig.step").exists());
  let raw = NewItem { project: &p, folder: None, name: "Fusion part", kind: ItemKind::File, ext: "f3d" };
  let (f3d, _) =
    s.create_item(raw, b"PK", Origin::Uploaded { file: "part.f3d".into() }, VersionMeta::default()).unwrap();
  assert!(s.version_path(&f3d, None).unwrap().ends_with("v1.f3d"));
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
