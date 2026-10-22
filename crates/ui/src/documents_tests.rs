//! Document tabs, "What do you want to design?", the Assembly Design document and its New
//! Component, and Fusion's Insert window, driven through a real egui context.

use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;
use fox_engine::model::DesignType;
use serde_json::json;

use crate::App;
use crate::documents::{self, NewKind, PartType};
use crate::insert_window::{self, Target};
use crate::panels::{Cmd, Tab};

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
  root: std::path::PathBuf,
}

impl Harness {
  fn new(tag: &str) -> Harness {
    let root = std::env::temp_dir().join(format!("fox-ui-docs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut app = App::with_session(Session::default());
    app.s.store_root = Some(root.join("hub"));
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness { ctx, app, time: 0.0, root };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  fn frame(&mut self, events: Vec<Event>) {
    self.time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    self.ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
  }

  fn key(&mut self, key: Key, modifiers: Modifiers) {
    let ev = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
    self.frame(vec![ev(true), ev(false)]);
    self.frame(vec![]);
  }

  fn area(&self, id: egui::Id) -> Option<Rect> {
    self.ctx.memory(|m| m.area_rect(id))
  }
}

impl Drop for Harness {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.root);
  }
}

/// Some unsaved work: a box and a user parameter.
fn work(app: &mut App) {
  app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  app.run("param.add", json!({"name": "kept", "expr": "5 mm"})).unwrap();
  assert!(app.s.dirty);
}

#[test]
fn create_new_adds_a_tab_and_keeps_the_unsaved_design() {
  let mut h = Harness::new("plus");
  work(&mut h.app);
  let first_revision = h.app.s.revision;
  // the "+" opens the chooser; Create New on Part Design
  documents::open_chooser(&mut h.app);
  h.frame(vec![]);
  assert!(h.area(crate::file_dialogs::modal_id("new-design")).is_some(), "What do you want to design? shows");
  let c = h.app.docs.chooser.unwrap();
  assert_eq!(c.kind, NewKind::Part);
  documents::create(&mut h.app, c);
  h.frame(vec![]);
  assert_eq!(h.app.docs.count(), 2);
  assert_eq!(h.app.docs.active(), 1);
  assert!(h.app.s.design.parameters.is_empty() && h.app.s.model.bodies.is_empty(), "the new tab is empty");
  assert!(!h.app.s.dirty);
  // the first design is still there, unsaved, in its own tab
  documents::switch_to(&mut h.app, 0);
  h.frame(vec![]);
  assert!(h.app.s.dirty);
  assert_eq!(h.app.s.design.parameters[0].name, "kept");
  assert_eq!(h.app.s.model.bodies.len(), 1);
  assert!(h.app.s.revision > first_revision, "the viewport rebuilds for the shown design");
  // and its undo history came with it
  assert!(h.app.s.can_undo());
}

#[test]
fn ctrl_n_and_file_new_never_drop_the_open_design() {
  let mut h = Harness::new("ctrl-n");
  work(&mut h.app);
  h.key(Key::N, Modifiers::COMMAND);
  assert_eq!(h.app.docs.count(), 2, "Ctrl+N adds a tab");
  assert!(h.app.s.design.parameters.is_empty());
  crate::panels::file_menu_action(&mut h.app, "New...");
  assert!(h.app.docs.chooser.is_some(), "File > New... asks what to design");
  assert_eq!(h.app.docs.count(), 2, "nothing is made until Create New");
  assert_eq!(documents::PLUS_TIP, "New Design");
}

#[test]
fn closing_a_tab_with_unsaved_changes_asks_first() {
  let mut h = Harness::new("close");
  work(&mut h.app);
  documents::new_document(&mut h.app, DesignType::Part, None);
  // the clean new tab closes at once, and the one on its left shows
  documents::request_close(&mut h.app, 1);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.s.dirty && h.app.s.design.parameters.len() == 1);
  // the unsaved one asks
  documents::request_close(&mut h.app, 0);
  assert_eq!(h.app.docs.closing, Some(0));
  h.frame(vec![]);
  assert!(h.area(crate::file_dialogs::modal_id("close-design")).is_some(), "Save changes? shows");
  assert!(h.app.s.dirty, "nothing is dropped while it asks");
  // Don't Save: the last tab closing leaves a new empty design
  documents::close_now(&mut h.app, 0);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.s.design.parameters.is_empty() && !h.app.s.dirty);
}

#[test]
fn opening_a_design_over_unsaved_work_opens_it_in_a_new_tab() {
  let mut h = Harness::new("open");
  // a saved design to open
  h.app.run("param.add", json!({"name": "saved", "expr": "1 mm"})).unwrap();
  let item = h.app.run("hub.save", json!({"name": "Saved"})).unwrap()["item"].as_str().unwrap().to_string();
  h.app.new_design();
  work(&mut h.app);
  h.app.open_hub_item(&item, None);
  assert_eq!(h.app.docs.count(), 2);
  assert_eq!(h.app.s.design.parameters[0].name, "saved");
  documents::switch_to(&mut h.app, 0);
  assert_eq!(h.app.s.design.parameters[0].name, "kept", "the unsaved design kept its tab");
  // a failed open takes the new tab back
  h.app.open_hub_item("no-such-item", None);
  assert_eq!(h.app.docs.count(), 2);
  assert_eq!(h.app.s.design.parameters[0].name, "kept");
  // a clean design is simply replaced, as before
  documents::switch_to(&mut h.app, 1);
  h.app.open_hub_item(&item, None);
  assert_eq!(h.app.docs.count(), 2);
}

#[test]
fn the_chooser_has_fusions_six_tiles() {
  let labels: Vec<&str> = NewKind::ALL.iter().map(|k| k.label()).collect();
  assert_eq!(
    labels,
    ["Part Design", "Assembly Design", "Hybrid Design", "Drawing", "Electronics Design", "Electronics Library"]
  );
  let greyed: Vec<NewKind> = NewKind::ALL.into_iter().filter(|k| k.unavailable().is_some()).collect();
  assert_eq!(greyed, [NewKind::Drawing, NewKind::ElectronicsLibrary]);
  assert_eq!(documents::unit_system_label(fox_engine::model::LengthUnit::In), "Inch (in), ouncemass");
  assert_eq!(documents::unit_system_label(fox_engine::model::LengthUnit::Mm), "Millimeter (mm), gram");
  // greyed tiles make nothing
  let mut h = Harness::new("tiles");
  let c = documents::Chooser { kind: NewKind::Drawing, part: PartType::Standard, unit: Default::default() };
  documents::create(&mut h.app, c);
  assert_eq!(h.app.docs.count(), 1);
}

#[test]
fn an_assembly_design_has_the_assembly_tab_and_document_type() {
  let mut h = Harness::new("assembly");
  let c =
    documents::Chooser { kind: NewKind::Assembly, part: PartType::Standard, unit: fox_engine::model::LengthUnit::In };
  documents::create(&mut h.app, c);
  h.frame(vec![]);
  assert_eq!(h.app.s.design.design_type, DesignType::Assembly);
  assert_eq!(h.app.s.design.units, fox_engine::model::LengthUnit::In, "the chooser's unit system");
  assert!(!h.app.s.can_undo(), "the settings aren't undo steps");
  assert_eq!(h.app.tab, Tab::Assembly);
  assert_eq!(h.app.s.design.design_type.label(), "Assembly Design");
  // a part design shows the modelling tabs again
  documents::new_document(&mut h.app, DesignType::Part, None);
  h.frame(vec![]);
  assert_eq!(h.app.tab, Tab::Solid);
  documents::switch_to(&mut h.app, 1);
  h.frame(vec![]);
  assert_eq!(h.app.tab, Tab::Assembly, "each tab keeps its toolbar tab");
  // a sheet metal part opens on SHEET METAL
  let c = documents::Chooser { kind: NewKind::Part, part: PartType::SheetMetal, unit: Default::default() };
  documents::create(&mut h.app, c);
  h.frame(vec![]);
  assert_eq!(h.app.tab, Tab::SheetMetal);
}

#[test]
fn new_component_in_an_assembly_makes_a_linked_external_design() {
  let mut h = Harness::new("external");
  documents::new_document(&mut h.app, DesignType::Assembly, None);
  h.frame(vec![]);
  Cmd::Dialog(crate::dialogs::Kind::NewComponent).run(&mut h.app);
  let d = h.app.asm_component.clone().expect("the assembly's NEW COMPONENT shows");
  assert_eq!(d.name, "External1");
  assert_eq!((d.design_type, d.part_type), ("part", "standard"));
  assert!(h.app.dialog.is_none(), "not the Part Design dialog");
  h.frame(vec![]);
  assert!(h.area(egui::Id::new("asm-new-component")).is_some());
  crate::asm_component::close(&mut h.app);
  crate::asm_component::create(&mut h.app, &d);
  let comp = h.app.s.design.components.last().unwrap_or_else(|| panic!("a component: {:?}", h.app.status));
  assert_eq!(comp.name, "External1");
  let items = h.app.s.execute("hub.search", &json!({"query": "External1"})).unwrap();
  assert_eq!(items.as_array().map(|a| a.len()), Some(1), "the external design is in the hub: {items}");
  // the next one is External2
  Cmd::Dialog(crate::dialogs::Kind::NewComponent).run(&mut h.app);
  assert_eq!(h.app.asm_component.as_ref().unwrap().name, "External2");
  h.app.escape();
  assert!(h.app.asm_component.is_none(), "Escape closes it");
  // a part design keeps Fusion's Part Design New Component
  documents::new_document(&mut h.app, DesignType::Part, None);
  Cmd::Dialog(crate::dialogs::Kind::NewComponent).run(&mut h.app);
  assert!(h.app.asm_component.is_none());
  assert!(h.app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::NewComponent));
}

#[test]
fn insert_entries_open_fusions_insert_window() {
  assert_eq!((Target::Decal.title(), Target::Decal.primary()), ("Insert", "Insert"));
  assert_eq!(Target::Svg.computer_button(), Some("Insert from my computer..."));
  assert_eq!((Target::Mesh.primary(), Target::Mesh.computer_button()), ("Select", Some("Select from my computer...")));
  assert_eq!(
    (Target::Derive.title(), Target::Derive.primary(), Target::Derive.computer_button()),
    ("Select Source", "Select", None)
  );
  let file = |ext: &str| json!({"kind": "file", "ext": ext});
  assert!(Target::Canvas.takes(&file("png")) && !Target::Canvas.takes(&file("svg")));
  let svg_upload = json!({"kind": "design", "ext": "fox", "latest": {"original": "v1-logo.orig.svg"}});
  assert!(Target::Svg.takes(&svg_upload) && !Target::Derive.takes(&svg_upload));
  assert!(Target::Derive.takes(&json!({"kind": "design", "ext": "fox"})) && !Target::Derive.takes(&file("png")));

  let mut h = Harness::new("insert");
  Cmd::Insert(Target::Svg).run(&mut h.app);
  h.frame(vec![]);
  assert!(h.area(crate::file_dialogs::modal_id("insert")).is_some(), "the Insert window shows");
  assert!(crate::file_dialogs::modal_open(&h.app), "the app's shortcuts wait");
  // an SVG from the hub goes into a new sketch
  let svg = h.root.join("logo.svg");
  std::fs::write(
    &svg,
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><path d="M0 0 L20 0 L20 10 Z"/></svg>"#,
  )
  .unwrap();
  let up = h.app.s.execute("hub.upload", &json!({"path": svg})).unwrap();
  let id = up["item"].as_str().unwrap().to_string();
  h.app.inserts.window = None;
  insert_window::pick_hub_item(&mut h.app, Target::Svg, &id);
  assert!(h.app.s.design.features.iter().any(|f| f.name == "logo"), "{:?}", h.app.status);
  // Canvas: the picked image opens the CANVAS form
  Cmd::Canvas.run(&mut h.app);
  assert_eq!(h.app.inserts.window.as_ref().map(|w| w.target), Some(Target::Canvas));
  // Insert DXF opens the docked dialog, not a file dialog
  Cmd::InsertDxf.run(&mut h.app);
  assert!(h.app.inserts.dxf.is_some());
  assert_eq!(insert_window::INSERT_MODES, ["Single Sketch", "One Sketch Per Layer"]);
}

#[test]
fn a_decal_goes_on_as_an_opaque_image() {
  let mut h = Harness::new("decal");
  let p = h.root.join("label.png");
  image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 20, 30, 255])).save(&p).unwrap();
  insert_window::insert_path(&mut h.app, Target::Decal, &p);
  let f = h.app.s.design.features.last().expect("a feature");
  assert_eq!(f.name, "label");
  assert!(matches!(&f.kind, fox_engine::model::FeatureKind::Canvas { opacity, .. } if *opacity == 1.0), "{:?}", f.kind);
}
