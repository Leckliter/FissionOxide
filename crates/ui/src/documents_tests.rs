//! Document tabs, "What do you want to design?", the Assembly Design document and its New
//! Component, and Fusion's Insert window, driven through a real egui context.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
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

  fn click(&mut self, pos: Pos2) {
    let button =
      |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    self.frame(vec![Event::PointerMoved(pos), button(true), button(false)]);
    self.frame(vec![]);
  }

  /// Frames until the window's activation grace is over (a click then counts as aimed).
  fn settle(&mut self) {
    for _ in 0..40 {
      self.frame(vec![]);
    }
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
  assert!(h.app.s.dirty, "nothing is dropped while it asks");
  // Don't Save: the last tab closing leaves a new empty design
  documents::close_now(&mut h.app, 0);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.s.design.parameters.is_empty() && !h.app.s.dirty);
  // a saved design with changes asks "Save changes?"
  h.app.run("param.add", json!({"name": "saved", "expr": "1 mm"})).unwrap();
  h.app.run("hub.save", json!({"name": "Saved"})).unwrap();
  h.app.run("param.add", json!({"name": "more", "expr": "2 mm"})).unwrap();
  documents::request_close(&mut h.app, 0);
  h.frame(vec![]);
  assert!(h.area(crate::file_dialogs::modal_id("close-design")).is_some(), "Save changes? shows");
  assert!(h.app.data.save.is_none());
}

/// Where the Save dialog's buttons are: (Don't Save, Cancel, Save).
fn save_buttons(h: &Harness) -> (Pos2, Pos2, Pos2) {
  use crate::file_dialogs::{BUTTON, MARGIN};
  let d = h.area(crate::file_dialogs::modal_id("save")).expect("the Save dialog shows");
  let y = d.bottom() - MARGIN - BUTTON.y / 2.0;
  let save = Pos2::new(d.right() - MARGIN - BUTTON.x / 2.0, y);
  let cancel = Pos2::new(save.x - BUTTON.x - 8.0, y);
  (Pos2::new(d.left() + MARGIN + BUTTON.x / 2.0, y), cancel, save)
}

#[test]
fn closing_a_never_saved_design_opens_the_save_dialog() {
  let mut h = Harness::new("close-save");
  work(&mut h.app);
  documents::new_document(&mut h.app, DesignType::Part, None);
  h.settle();
  // the X on the first, unsaved tab shows it and opens Save with Don't Save, not "Save changes?"
  documents::request_close(&mut h.app, 0);
  assert_eq!(h.app.docs.active(), 0, "the closing tab shows");
  assert_eq!(h.app.docs.closing, Some(0));
  let form = h.app.data.save.clone().expect("the Save dialog opens");
  assert_eq!(form.name, "Untitled");
  assert!(form.first, "the name is selected");
  h.frame(vec![]);
  assert!(h.area(crate::file_dialogs::modal_id("close-design")).is_none());
  assert!(h.app.docs.modal_open());
  h.frame(vec![]);
  // Cancel keeps the tab, changes and all
  let (_, cancel, _) = save_buttons(&h);
  h.click(cancel);
  assert!(h.app.data.save.is_none() && h.app.docs.closing.is_none());
  assert_eq!(h.app.docs.count(), 2);
  assert!(h.app.s.dirty);
  // Don't Save closes it
  documents::request_close(&mut h.app, 0);
  h.frame(vec![]);
  h.frame(vec![]);
  let (dont, _, _) = save_buttons(&h);
  h.click(dont);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.data.save.is_none() && h.app.docs.closing.is_none());
  assert!(h.app.s.design.parameters.is_empty(), "the other tab shows");
  // Save saves it to the hub, then closes it
  work(&mut h.app);
  documents::new_document(&mut h.app, DesignType::Part, None);
  documents::request_close(&mut h.app, 0);
  h.frame(vec![]);
  h.frame(vec![]);
  let (_, _, save) = save_buttons(&h);
  h.click(save);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.docs.closing.is_none());
  let recent = h.app.s.execute("hub.recent", &json!({})).unwrap();
  assert_eq!(recent.as_array().map(Vec::len), Some(1), "{recent}");
}

#[test]
fn tabs_star_unsaved_changes_and_number_new_designs() {
  let mut h = Harness::new("labels");
  assert_eq!(documents::tab_label(&h.app.s), "Untitled");
  work(&mut h.app);
  assert_eq!(documents::tab_label(&h.app.s), "Untitled*", "a never-saved design with changes is starred");
  // Ctrl+N numbers the new tabs, as Fusion's "Untitled(1)".."Untitled(4)"
  for n in 1..=4 {
    h.key(Key::N, Modifiers::COMMAND);
    assert_eq!(documents::tab_label(&h.app.s), format!("Untitled({n})"));
  }
  assert_eq!(h.app.title(), "Untitled(4) - FissionOxide", "the title is the tab's label and the product");
  // a closed number is the next one used
  documents::request_close(&mut h.app, 2);
  assert_eq!(h.app.docs.count(), 4);
  documents::new_document(&mut h.app, DesignType::Part, None);
  assert_eq!(h.app.s.design.name, "Untitled(2)");
  // the first save offers the tab's name
  h.app.save(false);
  assert_eq!(h.app.data.save.as_ref().map(|f| f.name.as_str()), Some("Untitled(2)"));
}

#[test]
fn an_overflowing_tab_strip_scrolls_and_lists_the_tabs() {
  assert!(!documents::tabs_overflow(4, 4.0 * documents::TAB_MIN_W));
  assert!(documents::tabs_overflow(5, 4.0 * documents::TAB_MIN_W));
  let mut h = Harness::new("overflow");
  for _ in 0..9 {
    h.key(Key::N, Modifiers::COMMAND);
  }
  assert_eq!(h.app.docs.count(), 10);
  h.frame(vec![]);
  // the "v" at the strip's end lists every tab; picking one shows it
  let v = h.app.docs.overflow_at.expect("the strip overflows and shows the v");
  assert!(!egui::Popup::is_any_open(&h.ctx));
  h.click(v.center());
  assert!(egui::Popup::is_any_open(&h.ctx), "the tab list opens");
  documents::switch_to(&mut h.app, 0);
  h.frame(vec![]);
  assert_eq!(documents::tab_label(&h.app.s), "Untitled");
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
fn every_opened_design_gets_its_own_tab_as_in_fusion() {
  let mut h = Harness::new("tabs-open");
  // two saved designs
  h.app.run("param.add", json!({"name": "a", "expr": "1 mm"})).unwrap();
  let a = h.app.run("hub.save", json!({"name": "A"})).unwrap()["item"].as_str().unwrap().to_string();
  h.app.new_design();
  h.app.run("param.add", json!({"name": "b", "expr": "2 mm"})).unwrap();
  let b = h.app.run("hub.save", json!({"name": "B"})).unwrap()["item"].as_str().unwrap().to_string();
  // the blank start-up design is replaced by the first design opened
  h.app.new_design();
  assert_eq!(h.app.docs.count(), 1);
  h.app.open_hub_item(&a, None);
  assert_eq!(h.app.docs.count(), 1, "a blank design's tab is reused");
  assert_eq!(h.app.s.design.parameters[0].name, "a");
  // a saved, unchanged design stays open: the next one gets a new tab
  h.app.open_hub_item(&b, None);
  assert_eq!(h.app.docs.count(), 2, "the open design keeps its tab");
  assert_eq!(h.app.s.design.parameters[0].name, "b");
  // opening a design that is already open shows its tab
  h.app.open_hub_item(&a, None);
  assert_eq!(h.app.docs.count(), 2);
  assert_eq!(h.app.docs.active(), 0);
  assert_eq!(h.app.s.design.parameters[0].name, "a");
  h.frame(vec![]);
}

#[test]
fn the_chooser_has_fusions_six_tiles() {
  let labels: Vec<&str> = NewKind::ALL.iter().map(|k| k.label()).collect();
  assert_eq!(
    labels,
    ["Part Design", "Assembly Design", "Hybrid Design", "Drawing", "Electronics Design", "Electronics Library"]
  );
  let greyed: Vec<NewKind> = NewKind::ALL.into_iter().filter(|k| k.unavailable().is_some()).collect();
  assert_eq!(greyed, [NewKind::ElectronicsLibrary]);
  assert_eq!(documents::unit_system_label(fox_engine::model::LengthUnit::In), "Inch (in), ouncemass");
  assert_eq!(documents::unit_system_label(fox_engine::model::LengthUnit::Mm), "Millimeter (mm), gram");
  // greyed tiles make nothing
  let mut h = Harness::new("tiles");
  let c = documents::Chooser { kind: NewKind::ElectronicsLibrary, ..Default::default() };
  documents::create(&mut h.app, c);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.dialog.is_none());
  // Drawing documents the shown design: Create Drawing opens on it, in the same tab
  let c = documents::Chooser { kind: NewKind::Drawing, automate: true, ..Default::default() };
  documents::create(&mut h.app, c);
  assert_eq!(h.app.docs.count(), 1);
  assert_eq!(h.app.dialog.as_ref().map(|d| d.kind), Some(crate::dialogs::Kind::NewDrawing));
}

#[test]
fn the_chooser_has_fusions_words_and_sizes() {
  assert_eq!(
    NewKind::Part.description(),
    "Create sketches and bodies to model a single component to manufacture or use in an assembly."
  );
  assert_eq!(
    NewKind::Assembly.description(),
    "Create or insert parts and other assemblies as external components, then define relationships between them \
     to create an assembly."
  );
  assert_eq!(
    NewKind::Hybrid.description(),
    "Create and model internal components, insert external components, and define relationships between them."
  );
  assert_eq!(
    NewKind::Drawing.description(),
    "Create views and annotations to document the active design for manufacturing."
  );
  assert_eq!(NewKind::Electronics.description(), "Create and manage schematics, 2D PCB, and 3D PCB documents.");
  assert_eq!(NewKind::ElectronicsLibrary.description(), "Create and manage components for your electronics designs.");
  let tips: Vec<(&str, &str)> = documents::PART_TYPES.iter().map(|(_, t, d)| (*t, *d)).collect();
  assert_eq!(
    tips,
    [
      ("Standard", "Creates a standard container for design elements like sketches and bodies."),
      (
        "Sheet Metal",
        "Creates a component that is subject to sheet metal rules and can be used to create flat patterns."
      ),
    ]
  );
  assert_eq!(documents::TILE, vec2(104.0, 145.0));
  // the window is about 640 x 413, as Fusion's
  let mut h = Harness::new("chooser-size");
  documents::open_chooser(&mut h.app);
  assert!(h.app.docs.chooser.is_some_and(|c| c.automate && c.kind == NewKind::Part));
  h.frame(vec![]);
  h.frame(vec![]);
  let r = h.area(crate::file_dialogs::modal_id("new-design")).expect("the chooser shows");
  assert!((r.width() - 640.0).abs() < 30.0 && (r.height() - 413.0).abs() < 40.0, "{r:?}");
}

#[test]
fn ctrl_tab_cycles_the_document_tabs() {
  let mut h = Harness::new("ctrl-tab");
  documents::new_document(&mut h.app, DesignType::Part, None);
  documents::new_document(&mut h.app, DesignType::Part, None);
  h.frame(vec![]);
  assert_eq!(h.app.docs.active(), 2);
  h.key(Key::Tab, Modifiers::COMMAND);
  assert_eq!(h.app.docs.active(), 0, "Ctrl+Tab wraps from the last tab to the first");
  h.key(Key::Tab, Modifiers::COMMAND);
  assert_eq!(h.app.docs.active(), 1);
  h.key(Key::Tab, Modifiers::COMMAND | Modifiers::SHIFT);
  assert_eq!(h.app.docs.active(), 0, "Ctrl+Shift+Tab goes back");
  h.key(Key::Tab, Modifiers::COMMAND | Modifiers::SHIFT);
  assert_eq!(h.app.docs.active(), 2, "and wraps to the last");
}

#[test]
fn tabs_keep_fusions_minimum_width_and_closing_the_last_shows_its_neighbour() {
  // about 150 px a tab before the strip scrolls, as in a 908 px Fusion window
  assert_eq!(documents::TAB_MIN_W, 150.0);
  assert!(!documents::tabs_overflow(4, 600.0));
  assert!(documents::tabs_overflow(5, 600.0));
  let mut h = Harness::new("overflow-close");
  for _ in 0..6 {
    documents::new_document(&mut h.app, DesignType::Part, None);
  }
  h.frame(vec![]);
  assert_eq!((h.app.docs.count(), h.app.docs.active()), (7, 6));
  // closing the shown rightmost tab shows its left neighbour
  documents::request_close(&mut h.app, 6);
  assert_eq!((h.app.docs.count(), h.app.docs.active()), (6, 5));
}

/// A frame whose input carries the window's X; returns the commands the app sent the window.
fn close_window(h: &mut Harness) -> Vec<egui::ViewportCommand> {
  h.time += 1.0 / 60.0;
  let mut input = RawInput {
    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
    time: Some(h.time),
    ..Default::default()
  };
  input.viewports.insert(
    egui::ViewportId::ROOT,
    egui::ViewportInfo { events: vec![egui::ViewportEvent::Close], ..Default::default() },
  );
  let app = &mut h.app;
  let mut out = h.ctx.run_ui(input, |ui| app.draw(ui));
  out.textures_delta.clear();
  out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default()
}

fn frame_commands(h: &mut Harness) -> Vec<egui::ViewportCommand> {
  h.time += 1.0 / 60.0;
  let input = RawInput {
    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
    time: Some(h.time),
    ..Default::default()
  };
  let app = &mut h.app;
  let mut out = h.ctx.run_ui(input, |ui| app.draw(ui));
  out.textures_delta.clear();
  out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default()
}

#[test]
fn the_windows_x_asks_to_save_unsaved_designs() {
  use egui::ViewportCommand;
  let mut h = Harness::new("window-close");
  // nothing unsaved: the window just closes
  let cmds = close_window(&mut h);
  assert!(!cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
  // an unsaved design in the second tab, the first shown: the close waits and that tab asks
  documents::new_document(&mut h.app, DesignType::Part, None);
  work(&mut h.app);
  documents::switch_to(&mut h.app, 0);
  let cmds = close_window(&mut h);
  assert!(cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
  assert!(h.app.docs.quitting);
  assert_eq!((h.app.docs.closing, h.app.docs.active()), (Some(1), 1), "the unsaved tab shows and asks");
  assert!(h.app.data.save.is_some(), "with the Save dialog, as its tab's X");
  // the Save dialog's Cancel keeps the app open with every tab as it was
  h.app.data.save = None;
  h.app.docs.closing = None;
  let cmds = frame_commands(&mut h);
  assert!(!cmds.contains(&ViewportCommand::Close), "{cmds:?}");
  assert!(!h.app.docs.quitting);
  assert_eq!(h.app.docs.count(), 2);
  assert!(h.app.s.dirty, "the work is still there");
  // again, and Don't Save: the tab goes and the window closes
  close_window(&mut h);
  assert_eq!(h.app.docs.closing, Some(1));
  h.app.data.save = None;
  documents::close_now(&mut h.app, 1);
  let cmds = frame_commands(&mut h);
  assert!(cmds.contains(&ViewportCommand::Close), "{cmds:?}");
  assert!(!h.app.docs.quitting);
}

#[test]
fn an_assembly_design_has_the_assembly_tab_and_document_type() {
  let mut h = Harness::new("assembly");
  let c = documents::Chooser { kind: NewKind::Assembly, unit: fox_engine::model::LengthUnit::In, ..Default::default() };
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
  let c = documents::Chooser { kind: NewKind::Part, part: PartType::SheetMetal, ..Default::default() };
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

/// Where each text drawn this frame starts, by its text (the tests find labels with it).
fn texts(h: &mut Harness) -> Vec<(String, Pos2)> {
  h.time += 1.0 / 60.0;
  let input = RawInput {
    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
    time: Some(h.time),
    ..Default::default()
  };
  let app = &mut h.app;
  let mut out = h.ctx.run_ui(input, |ui| app.draw(ui));
  out.textures_delta.clear();
  out
    .shapes
    .iter()
    .filter_map(|c| match &c.shape {
      egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos)),
      _ => None,
    })
    .collect()
}

/// The Insert window's PROJECT column lists the hub's projects (Default Project picked) under
/// its heading, each once; a click on one shows its files.
#[test]
fn the_insert_window_lists_the_projects_in_their_column() {
  let mut h = Harness::new("insert-projects");
  for name in ["Default Project", "Other"] {
    h.app.s.execute("hub.project.create", &json!({"name": name})).unwrap();
  }
  Cmd::Insert(Target::Svg).run(&mut h.app);
  h.frame(vec![]);
  assert!(h.area(crate::file_dialogs::modal_id("insert")).is_some(), "the Insert window shows");
  let shown = texts(&mut h);
  let heading = shown.iter().find(|(t, _)| t == "PROJECT").expect("the PROJECT heading").1;
  // the column is 220 wide, its texts 10 in from its left
  let column = heading.x + 210.0;
  let other = shown.iter().find(|(t, _)| t == "Other").expect("the other project is listed").1;
  assert!(other.x < column && other.y > heading.y + 10.0, "under the heading, in the column: {other:?} {heading:?}");
  let defaults: Vec<Pos2> = shown.iter().filter(|(t, _)| t == "Default Project").map(|p| p.1).collect();
  assert_eq!(defaults.len(), 2, "the list row and the file table's heading: {defaults:?}");
  assert!(defaults.iter().filter(|p| p.x < column).count() == 1, "one in the column: {defaults:?}");
  let at = other + vec2(4.0, 6.0);
  h.frame(vec![Event::PointerMoved(at)]);
  h.frame(vec![
    Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
    Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
  ]);
  h.frame(vec![]);
  let w = h.app.inserts.window.as_ref().expect("still open");
  let picked = w.projects.iter().find(|p| p["id"].as_str() == w.project.as_deref()).map(|p| p["name"].clone());
  assert_eq!(picked, Some(json!("Other")));
}

/// A click at `at`.
fn click(h: &mut Harness, at: Pos2) {
  h.frame(vec![Event::PointerMoved(at)]);
  h.frame(vec![
    Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
    Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
  ]);
  h.frame(vec![]);
}

/// The house shows the Home view in place of the design (Fusion's title, Start working, Recent
/// with the hub's designs, Ways to explore; no toolbar); a document's tab goes back to it.
#[test]
fn the_house_opens_home_and_a_tab_goes_back() {
  let mut h = Harness::new("home");
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  h.app.s.execute("hub.save", &json!({"name": "Bracket"})).unwrap();
  h.frame(vec![]);
  // the first icon of the application bar is the house
  click(&mut h, egui::pos2(8.0 + 13.0, 17.0));
  assert!(h.app.home.open, "the house opens Home");
  assert_eq!(h.app.title(), "Home - FissionOxide");
  let shown = texts(&mut h);
  let has = |t: &str| shown.iter().any(|(s, _)| s == t);
  for t in
    ["Home", "My FissionOxide", "Projects", "Samples", "Start working", "+ New Design", "Recent", "Ways to explore"]
  {
    assert!(has(t), "{t} shows");
  }
  for c in crate::home::COLUMNS {
    assert!(has(c), "Recent's {c} column");
  }
  assert!(has("Bracket"), "the saved design is in Recent");
  assert!(!has("SOLID") && !has("Bodies"), "no toolbar or browser in place of the design");
  // a click on the document's tab goes back to the design
  let tab = shown.iter().find(|(s, _)| s.starts_with("Bracket v")).expect("the document's tab").1;
  click(&mut h, tab + vec2(4.0, 6.0));
  assert!(!h.app.home.open, "the tab shows the design again");
  assert!(h.app.title().starts_with("Bracket"));
  // Start working's New Part Design leaves Home for a new design
  crate::home::open(&mut h.app);
  crate::home::act(&mut h.app, crate::home::Act::Start("New Part Design"));
  assert!(!h.app.home.open);
  assert_eq!(h.app.docs.count(), 2, "in a tab of its own");
}

/// The profile menu has Fusion's entries; Preferences opens the Preferences dialog.
#[test]
fn the_profile_menu_lists_fusions_entries() {
  let labels: Vec<Option<&str>> = crate::panels::PROFILE_MENU.iter().map(|e| e.map(|(l, _)| l)).collect();
  assert_eq!(
    labels,
    [
      Some("Autodesk Account"),
      Some("Preferences"),
      Some("My Profile"),
      Some("Share Feedback"),
      None,
      Some("Subscribe Now"),
      None,
      Some("Sign Out")
    ]
  );
  let enabled: Vec<&str> = crate::panels::PROFILE_MENU.iter().flatten().filter(|e| e.1).map(|e| e.0).collect();
  assert_eq!(enabled, ["Preferences", "Share Feedback"], "the account entries are greyed");
  let mut h = Harness::new("profile");
  // the menu is a narrow list under the avatar, as Fusion's (about 108 screenshot px)
  let avatar = texts(&mut h).into_iter().find(|(t, _)| t == "CL").expect("the avatar").1;
  click(&mut h, avatar + vec2(2.0, 2.0));
  let shown = texts(&mut h);
  assert!(shown.iter().any(|(t, _)| t == "Autodesk Account"), "the menu opens");
  let menu = h.ctx.data(|d| d.get_temp::<Rect>(crate::panels::profile_menu_rect_id())).expect("the menu's rect");
  assert!(menu.width() <= crate::panels::PROFILE_MENU_W + 1.0, "narrow: {menu:?}");
  assert!(menu.left() < avatar.x && menu.right() > avatar.x - 150.0, "under the avatar: {menu:?} {avatar:?}");
  for (t, at) in shown.iter().filter(|(t, _)| t == "Autodesk Account" || t == "Subscribe Now") {
    assert!(menu.contains(*at), "{t} inside the menu: {at:?} {menu:?}");
  }
  crate::panels::profile_action(&mut h.app, "Preferences");
  assert!(h.app.preferences.open, "Preferences opens");
}

/// The Home page shows nothing of the design underneath: no Unsaved banner over its cards.
#[test]
fn home_hides_the_unsaved_banner() {
  let mut h = Harness::new("home-banner");
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  assert!(h.app.s.dirty && h.app.s.path.is_none());
  // a frame first: a newly shown area draws nothing while it sizes itself
  let unsaved = |h: &mut Harness| {
    h.frame(vec![]);
    texts(h).iter().any(|(t, _)| t == "Unsaved:")
  };
  assert!(unsaved(&mut h), "the design shows the banner");
  crate::home::open(&mut h.app);
  assert!(!unsaved(&mut h), "Home doesn't");
}
