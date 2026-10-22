//! The File menu and its windows, driven through a real egui context: Fusion's entries,
//! separators and tooltips, File ▸ View, the Open, Export and Save windows, 3D PRINT, Image
//! Options, and which windows Escape closes.

use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;
use serde_json::json;

use crate::App;
use crate::capture::{ImageUnit, Preparation};
use crate::file_dialogs::{self, EXPORT_TYPES, Pick};
use crate::panels::{FILE_MENU, FILE_MENU_BREAKS, file_menu_action};

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
  root: std::path::PathBuf,
}

impl Harness {
  fn new(tag: &str) -> Harness {
    let root = std::env::temp_dir().join(format!("fox-ui-files-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
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

  fn escape(&mut self) {
    self.key(Key::Escape, Modifiers::NONE);
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

#[test]
fn the_file_menu_has_fusions_separators_tooltips_and_a_greyed_save_as() {
  let before: Vec<&str> = FILE_MENU_BREAKS.iter().map(|i| FILE_MENU[*i].0).collect();
  assert_eq!(before, ["Save", "Create Version", "3D Print", "View Details on Web"]);
  for (label, ..) in FILE_MENU {
    assert!(crate::file_menu::file_tip(label).is_some(), "{label} has a tooltip");
  }
  assert_eq!(
    crate::file_menu::file_tip("Open..."),
    Some((
      "Opens a design or drawing from your projects or local computer.",
      "Select a design or drawing from the project browser and click Open, or click 'Open from my computer' and \
       select a supported file."
    ))
  );
  assert_eq!(crate::tips::MORE_HELP, "Press Ctrl+/ for more help.");
  // Save As waits for the first save
  let mut h = Harness::new("menu");
  assert!(!crate::file_menu::enabled(&h.app, "Save As", true));
  h.app.run("hub.save", json!({"name": "Bracket", "new": true, "explicit": true})).expect("saved");
  assert!(crate::file_menu::enabled(&h.app, "Save As", true));
  assert!(!crate::file_menu::enabled(&h.app, "Share File Link", false));
  // Export... is one entry that opens the Export window
  file_menu_action(&mut h.app, "Export...");
  assert!(h.app.files.export.is_some());
}

#[test]
fn file_view_lists_fusions_hide_and_show_entries_with_their_shortcuts() {
  let mut h = Harness::new("view");
  h.app.data.open = false;
  let labels = |app: &App| crate::file_menu::view_entries(app);
  let entries = labels(&h.app);
  let text: Vec<String> = entries.iter().map(|(l, k, _)| format!("{l} {k}")).collect();
  assert_eq!(
    text,
    [
      "Hide ViewCube Ctrl+Alt+V",
      "Hide Browser Ctrl+Alt+B",
      "Hide Comments Ctrl+Alt+A",
      "Show Text Commands Ctrl+Alt+C",
      "Hide Navigation Bar Ctrl+Alt+N",
      "Hide Command Toolbar Ctrl+Alt+T",
      "Show Data Panel Ctrl+Alt+P",
      "Show Performance Overlay Ctrl+Alt+O",
      "Reset to Default Layout Ctrl+Alt+R",
    ]
  );
  assert!(entries.iter().all(|e| e.2), "every entry runs, Text Commands too");
  h.key(Key::C, Modifiers::COMMAND | Modifiers::ALT);
  assert!(h.app.text_commands.open, "Ctrl+Alt+C shows the TEXT COMMANDS panel");
  assert_eq!(labels(&h.app)[3].0, "Hide Text Commands");
  h.key(Key::C, Modifiers::COMMAND | Modifiers::ALT);
  assert!(!h.app.text_commands.open);
  // the shortcuts flip them, and the labels with them
  let ctrl_alt = Modifiers::COMMAND | Modifiers::ALT;
  h.key(Key::V, ctrl_alt);
  assert!(!h.app.show_viewcube);
  assert_eq!(labels(&h.app)[0].0, "Show ViewCube");
  h.key(Key::T, ctrl_alt);
  assert!(!h.app.show_toolbar);
  h.key(Key::P, ctrl_alt);
  assert!(h.app.data.open);
  h.key(Key::O, ctrl_alt);
  assert!(h.app.show_perf);
  h.key(Key::B, ctrl_alt);
  assert!(!h.app.browser_open);
  h.key(Key::R, ctrl_alt);
  assert!(h.app.show_viewcube && h.app.show_toolbar && h.app.browser_open);
  assert!(!h.app.data.open && !h.app.show_perf);
}

#[test]
fn open_shows_fusions_window_with_the_projects_samples_and_designs() {
  let mut h = Harness::new("open");
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
  let saved = h.app.run("hub.save", json!({"name": "Bracket", "new": true, "explicit": true})).expect("saved");
  let item = saved["item"].as_str().unwrap_or_default().to_string();
  h.app.new_design();
  h.app.data.open = false;
  h.frame(vec![]);
  h.key(Key::O, Modifiers::COMMAND);
  let f = h.app.files.open.as_ref().expect("Ctrl+O shows the Open window, not the computer's");
  assert_eq!(f.projects.len(), 1);
  assert!(f.project.is_some(), "the project is picked");
  let names: Vec<&str> =
    f.listing["items"].as_array().into_iter().flatten().filter_map(|i| i["name"].as_str()).collect();
  assert_eq!(names, ["Bracket"]);
  assert_eq!(file_dialogs::SAMPLES.len(), 6);
  let rect = h.area(file_dialogs::modal_id("open")).expect("the window shows");
  assert!(rect.width() > 700.0, "{rect:?}");
  // Escape doesn't close it (Cancel or the X do, as in Fusion)
  h.escape();
  assert!(h.app.files.open.is_some());
  // New Project in the tree
  file_dialogs::create_project(&mut h.app, "Fixtures");
  file_dialogs::refresh_open(&mut h.app);
  let f = h.app.files.open.as_ref().unwrap();
  assert_eq!(f.projects.len(), 2);
  assert!(f.listing["items"].as_array().is_none_or(Vec::is_empty), "the new project is picked and empty");
  // open the design: pick it and click Open
  let open = rect.right_bottom() - vec2(file_dialogs::MARGIN + 40.0, file_dialogs::MARGIN + 13.0);
  let f = h.app.files.open.as_mut().unwrap();
  f.project = Some(saved["project"].as_str().unwrap_or_default().to_string());
  f.selected = Some(Pick::Item(item.clone()));
  h.frame(vec![Event::PointerMoved(open)]);
  let b = |pressed| Event::PointerButton {
    pos: open,
    button: egui::PointerButton::Primary,
    pressed,
    modifiers: Modifiers::NONE,
  };
  h.frame(vec![b(true), b(false)]);
  h.frame(vec![]);
  assert!(h.app.files.open.is_none(), "{:?}", h.app.status);
  assert_eq!(h.app.s.hub.as_ref().map(|l| l.item.clone()), Some(item));
}

#[test]
fn export_is_fusions_window_with_name_type_and_location() {
  let mut h = Harness::new("export");
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
  file_menu_action(&mut h.app, "Export...");
  let mut form = h.app.files.export.clone().expect("open");
  assert_eq!(form.name, h.app.s.design.name);
  assert!(form.first, "the name is selected");
  assert_eq!(EXPORT_TYPES[form.kind].label, "FissionOxide Archive Files (*.foxz)");
  let fusion: Vec<&str> = EXPORT_TYPES[1..15].iter().map(|t| t.label).collect();
  assert_eq!(
    fusion,
    [
      "Autodesk Fusion Archive Files (*.f3d)",
      "3MF Files (*.3mf)",
      "Autodesk Inventor 2025 Files (*.ipt)",
      "DWG Files (*.dwg)",
      "DXF Files (*.dxf)",
      "FBX Files (*.fbx)",
      "IGES Files (*.igs *.iges)",
      "OBJ Files (*.obj)",
      "SAT Files (*.sat)",
      "SketchUp Files (*.skp)",
      "SMT Files (*.smt)",
      "STEP Files (*.stp *.step)",
      "STL Files (*.stl)",
      "USD Files (*.usdz)",
    ]
  );
  let fusion_greys: Vec<&str> = EXPORT_TYPES.iter().filter(|t| t.fusion_greys).map(|t| t.ext).collect();
  assert_eq!(fusion_greys, ["dwg", "dxf", "igs", "sat"]);
  for t in EXPORT_TYPES {
    assert!(t.fusion_greys <= t.format.is_none(), "{} is greyed as in Fusion", t.label);
    if let Some(id) = t.format {
      assert!(fox_engine::exchange::FORMATS.iter().any(|f| f.id == id && f.export), "{id} is writable");
    }
  }
  h.frame(vec![]);
  assert!(h.area(file_dialogs::modal_id("export")).is_some(), "the window shows");
  h.escape();
  assert!(h.app.files.export.is_some(), "Escape doesn't close Export");
  // write an STL into a folder
  let dir = h.root.join("out");
  std::fs::create_dir_all(&dir).unwrap();
  form.location = dir.display().to_string();
  form.name = "Part".into();
  form.kind = EXPORT_TYPES.iter().position(|t| t.ext == "stl").unwrap();
  assert_eq!(form.path(), dir.join("Part.stl"));
  assert!(file_dialogs::export_now(&mut h.app, &form), "{:?}", h.app.status);
  assert!(dir.join("Part.stl").is_file());
  // a greyed type or a missing folder writes nothing
  form.kind = 1;
  assert!(!file_dialogs::export_now(&mut h.app, &form));
  form.kind = 0;
  form.location = h.root.join("nowhere").display().to_string();
  assert!(!file_dialogs::export_now(&mut h.app, &form));
}

#[test]
fn escape_closes_the_prompts_image_options_and_3d_print_but_not_save() {
  let mut h = Harness::new("escape");
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
  h.app.data.open = true;
  crate::capture::open_print(&mut h.app);
  crate::capture::open_image(&mut h.app);
  h.app.data.ask_connect();
  h.frame(vec![]);
  h.frame(vec![]);
  assert!(h.area(egui::Id::new(crate::capture::PRINT_AREA)).is_some(), "3D PRINT shows");
  assert!(h.area(file_dialogs::floating_id("image-options")).is_some(), "Image Options shows");
  h.escape();
  assert!(!h.app.data.prompt_open(), "the prompt goes first");
  assert!(h.app.capture.image.is_some() && h.app.capture.print.is_some());
  h.escape();
  assert!(h.app.capture.image.is_none());
  assert!(h.app.capture.print.is_some());
  h.escape();
  assert!(h.app.capture.print.is_none());
  // the modal Save closes with Cancel or the X only
  h.app.save(false);
  h.frame(vec![]);
  h.escape();
  assert!(h.app.data.save.is_some());
}

#[test]
fn the_first_save_asks_for_name_and_location_only() {
  let mut h = Harness::new("save");
  h.app.save(false);
  let f = h.app.data.save.clone().unwrap();
  assert!(f.first, "the name is selected when it shows");
  h.frame(vec![]);
  h.frame(vec![]);
  let first = h.area(file_dialogs::modal_id("save")).expect("Save shows");
  assert!(!h.app.data.save.as_ref().unwrap().first);
  // a later save asks for a description and milestone instead
  h.app.data.save = None;
  h.app.run("hub.save", json!({"name": "Bracket", "new": true, "explicit": true})).expect("saved");
  h.app.save(false);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let later = h.area(file_dialogs::modal_id("save")).expect("Save version shows");
  assert!((first.width() - later.width()).abs() < 1.0, "{first:?} {later:?}");
  assert!(first.height() < 280.0, "Name, Location and the buttons: {first:?}");
}

#[test]
fn three_d_print_docks_with_preparation_type_manufacture() {
  let mut h = Harness::new("print");
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
  h.app.start(crate::dialogs::Kind::Fillet);
  file_menu_action(&mut h.app, "3D Print");
  assert!(h.app.dialog.is_none(), "3D PRINT takes the command dialog's place");
  let form = h.app.capture.print.clone().unwrap();
  assert_eq!(form.preparation, Preparation::Manufacture);
  h.frame(vec![]);
  h.frame(vec![]);
  let rect = h.area(egui::Id::new(crate::capture::PRINT_AREA)).expect("docked");
  let dock = crate::panels::dialog_dock(h.app.canvas, false);
  assert!((rect.right() - dock.x).abs() < 2.0 && (rect.top() - dock.y).abs() < 2.0, "{rect:?} {dock:?}");
  assert!(rect.height() < 260.0, "sized to its rows: {rect:?}");
  // OK goes to the Manufacture workspace
  crate::capture::print_ok(&mut h.app, &form);
  assert!(h.app.manufacture);
  assert!(h.app.capture.print.is_none());
  // a command started from the toolbar closes it
  h.app.manufacture = false;
  crate::capture::open_print(&mut h.app);
  h.app.start(crate::dialogs::Kind::Fillet);
  h.frame(vec![]);
  assert!(h.app.capture.print.is_none());
}

#[test]
fn image_options_shows_the_window_size_and_converts_custom_units() {
  let mut h = Harness::new("image");
  h.app.canvas = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 426.0));
  h.app.canvas_px = [1200, 640];
  crate::capture::open_image(&mut h.app);
  let mut f = h.app.capture.image.clone().unwrap();
  assert_eq!(f.points, [800, 426], "the fields show the window's size");
  assert_eq!(f.size(), [1200, 640], "W: and H: show its pixels");
  assert_eq!((f.ppi, f.unit, f.lock, f.transparent, f.antialias), (72.0, ImageUnit::Pixels, true, false, true));
  // custom, in inches at 72 pixels per inch, with the aspect ratio locked
  f.window_size = false;
  f.unit = ImageUnit::Inches;
  assert!((f.in_unit(1200) - 1200.0 / 72.0).abs() < 1e-9);
  let px = f.px_of(10.0);
  f.set_width(px);
  assert_eq!(f.size(), [720, 384]);
  f.lock = false;
  f.unit = ImageUnit::Millimeters;
  let px = f.px_of(25.4);
  f.set_height(px);
  assert_eq!(f.size(), [720, 72]);
  h.frame(vec![]);
  h.frame(vec![]);
  assert!(h.area(file_dialogs::floating_id("image-options")).is_some_and(|r| r.height() < 380.0));
}
