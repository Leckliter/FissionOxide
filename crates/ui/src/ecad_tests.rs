//! The electronics documents, driven through a real egui context: DESIGN ▸ ELECTRONICS, the
//! electronics design's own tab and start page, and the schematic editor's ribbon and layout.

use egui::{Event, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;
use serde_json::json;

use crate::App;
use crate::documents;
use crate::ecad_chrome::{self, DocKind, Page};
use crate::ecad_ui::{Sheet, Tool};

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
}

impl Harness {
  fn new() -> Harness {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness { ctx, app: App::with_session(Session::default()), time: 0.0 };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  /// A frame with `events`, returning every text drawn with where.
  fn frame(&mut self, events: Vec<Event>) -> Vec<(Rect, String)> {
    self.time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    let out = self.ctx.run_ui(input, |ui| app.draw(ui));
    let t = texts(&out);
    out.drop_without_applying_deltas();
    t
  }

  /// The texts of a frame (after one to settle the layout).
  fn texts(&mut self) -> Vec<String> {
    self.frame(vec![]);
    self.frame(vec![]).into_iter().map(|(_, t)| t).collect()
  }

  fn find(&mut self, text: &str) -> Option<Rect> {
    self.frame(vec![]);
    self.frame(vec![]).into_iter().find(|(_, t)| t == text).map(|(r, _)| r)
  }

  fn click(&mut self, pos: Pos2) {
    let button = |pressed| Event::PointerButton {
      pos,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    self.frame(vec![Event::PointerMoved(pos)]);
    self.frame(vec![button(true)]);
    self.frame(vec![button(false)]);
    self.frame(vec![]);
  }

  /// Clicks the single icon above a ribbon panel's name.
  fn click_panel(&mut self, name: &str) {
    let r = self.find(name).unwrap_or_else(|| panic!("the {name} panel shows"));
    self.click(Pos2::new(r.center().x - 4.0, r.top() - 22.0));
  }
}

fn texts(out: &egui::FullOutput) -> Vec<(Rect, String)> {
  fn walk(s: &egui::Shape, out: &mut Vec<(Rect, String)>) {
    match s {
      egui::Shape::Text(t) => out.push((t.visual_bounding_rect(), t.galley.text().to_string())),
      egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
      _ => {}
    }
  }
  let mut all = Vec::new();
  for c in &out.shapes {
    walk(&c.shape, &mut all);
  }
  all
}

fn work(app: &mut App) {
  app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  assert!(app.s.dirty);
}

fn page(h: &Harness) -> Page {
  h.app.electronics.as_ref().expect("an electronics document shows").page
}

#[test]
fn the_electronics_submenu_has_fusions_two_rows() {
  let mut h = Harness::new();
  work(&mut h.app);
  let switcher = h.find("DESIGN").expect("the workspace switcher");
  h.click(switcher.center());
  let ecad = h.find("ELECTRONICS").expect("ELECTRONICS is in the menu");
  h.frame(vec![Event::PointerMoved(ecad.center())]);
  let shown = h.texts();
  assert!(shown.iter().any(|t| t == "New Electronics Design"), "{shown:?}");
  assert!(shown.iter().any(|t| t == "New Electronics Library"), "{shown:?}");
  for gone in ["New Schematic", "New PCB", "New 3D PCB"] {
    assert!(!shown.iter().any(|t| t == gone), "{gone} isn't in Fusion's submenu");
  }
  // the library row is greyed: FissionOxide has no electronics libraries
  assert!(crate::panels::workspace_document_unavailable("New Electronics Library").is_some());
  let lib = h.find("New Electronics Library").unwrap();
  h.click(lib.center());
  assert_eq!(h.app.docs.count(), 1, "the greyed row makes nothing");
  // New Electronics Design: a new tab, the part design left as it was
  if h.find("New Electronics Design").is_none() {
    let switcher = h.find("DESIGN").unwrap();
    h.click(switcher.center());
    let ecad = h.find("ELECTRONICS").unwrap();
    h.frame(vec![Event::PointerMoved(ecad.center())]);
  }
  let new = h.find("New Electronics Design").expect("the submenu");
  h.click(new.center());
  assert_eq!(h.app.docs.count(), 2);
  assert_eq!(h.app.docs.active(), 1);
  assert_eq!(page(&h), Page::Start);
}

#[test]
fn a_new_electronics_design_has_its_own_tab() {
  let mut h = Harness::new();
  work(&mut h.app);
  ecad_chrome::new_electronics_design(&mut h.app, None);
  assert_eq!((h.app.docs.count(), h.app.docs.active()), (2, 1));
  // "Untitled" beside the part design's "Untitled*": Fusion numbers electronics documents apart
  assert_eq!(documents::tab_label(&h.app.s), "Untitled");
  assert_eq!(ecad_chrome::doc_kind(h.app.electronics.as_ref()), DocKind::Electronics);
  // the part design's tab is as it was, in the Design workspace
  documents::switch_to(&mut h.app, 0);
  assert!(h.app.electronics.is_none());
  assert_eq!(h.app.s.model.bodies.len(), 1);
  let shown = h.texts();
  assert!(shown.iter().any(|t| t == "DESIGN"), "the part design keeps its workspace dropdown");
  // and the electronics tab comes back on its start page
  documents::switch_to(&mut h.app, 1);
  assert_eq!(page(&h), Page::Start);
  // closing it asks Save / Don't Save / Cancel, naming it "Electronics Design"
  documents::request_close(&mut h.app, 1);
  assert_eq!(h.app.docs.closing, Some(1));
  let shown = h.texts();
  assert!(shown.iter().any(|t| t == "Save changes to Electronics Design?"), "{shown:?}");
  documents::close_now(&mut h.app, 1);
  assert_eq!(h.app.docs.count(), 1);
  assert!(h.app.electronics.is_none() && h.app.s.model.bodies.len() == 1, "the part design shows again");
}

#[test]
fn the_start_page_is_fusions() {
  let mut h = Harness::new();
  ecad_chrome::new_electronics_design(&mut h.app, None);
  let shown = h.texts();
  for t in [
    "COMMON",
    "UTILITIES",
    "CREATE",
    "IMPORT",
    "OUTPUTS",
    "BROWSER",
    "(Unsaved)",
    "Description",
    "Edit",
    ecad_chrome::START_HEADING,
    ecad_chrome::START_LEAD,
    "New Schematic",
    "Import Non-Fusion File",
    "Learning Resources",
    "TEXT COMMANDS",
    "Txt",
    "Py",
    "TS",
  ] {
    assert!(shown.iter().any(|s| s == t), "{t} is on the start page: {shown:?}");
  }
  assert!(!shown.iter().any(|s| s == "DESIGN" || s == "ELECTRONICS"), "no workspace dropdown: {shown:?}");
  // Edit sits under the Description's box
  let (text, edit) = (h.find("No description").unwrap(), h.find("Edit").unwrap());
  assert!(edit.top() > text.top() + 80.0, "{text:?} {edit:?}");
  // CREATE: five commands, From Design greyed as in Fusion; the references need Fusion's hub
  let v = h.app.electronics.clone().unwrap();
  let create = &ecad_chrome::ribbon_panels(&v)[0];
  assert_eq!(create.0, "CREATE");
  let labels: Vec<&str> = create.1.iter().map(|c| c.label).collect();
  assert_eq!(
    labels,
    ["New Schematic", "Reference a Schematic document", "New PCB", "Reference a PCB document", "From Design"]
  );
  assert!(!create.1[4].on, "From Design is greyed");
  // the blue New Schematic opens the editor in another tab
  let button = h.find("New Schematic").unwrap();
  h.click(button.center());
  assert_eq!((h.app.docs.count(), h.app.docs.active()), (3, 2));
  assert_eq!(page(&h), Page::Editor);
  assert_eq!(h.app.electronics.as_ref().unwrap().sheet, Sheet::Schematic);
  documents::switch_to(&mut h.app, 1);
  assert_eq!(page(&h), Page::Start, "the electronics design stays on its start page");
}

#[test]
fn the_schematic_ribbon_is_icon_panels_under_fusions_tabs() {
  let mut h = Harness::new();
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  let shown = h.texts();
  let panels =
    ["SWITCH", "VIEW", "EDIT", "LAYERS", "PLACE", "CONNECT", "SIMULATE", "REWORK", "MODIFY", "SHORTCUTS", "SELECT"];
  for t in ecad_chrome::EDITOR_TABS.iter().chain(&panels) {
    assert!(shown.iter().any(|s| s == t), "{t} is on the ribbon: {shown:?}");
  }
  let v = h.app.electronics.clone().unwrap();
  let names: Vec<&str> = ecad_chrome::ribbon_panels(&v).iter().map(|p| p.0).collect();
  assert_eq!(names, panels);
  // no text buttons
  for gone in ["SCHEMATIC", "BOARD", "Wire", "No-connect", "Import library…", "New electronics design"] {
    assert!(!shown.iter().any(|s| s == gone), "{gone} is gone: {shown:?}");
  }
  // the icons work: Net, then SELECT; SWITCH goes to the board
  ecad_chrome::run_cmd(&mut h.app, ecad_chrome::NET);
  assert_eq!(h.app.electronics.as_ref().unwrap().tool, Tool::Wire);
  h.click_panel("SELECT");
  assert_eq!(h.app.electronics.as_ref().unwrap().tool, Tool::Select);
  h.click_panel("SWITCH");
  assert_eq!(h.app.electronics.as_ref().unwrap().sheet, Sheet::Board);
  assert!(h.texts().iter().any(|s| s == "PCB"), "the browser shows the PCB");
  // VALIDATE: the rules check
  let r = h.find("VALIDATE").unwrap();
  h.click(r.center());
  assert_eq!(h.app.electronics.as_ref().unwrap().ribbon, 2);
}

#[test]
fn the_schematic_editor_has_fusions_layout() {
  let mut h = Harness::new();
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  let all = {
    h.frame(vec![]);
    h.frame(vec![])
  };
  let shown: Vec<&str> = all.iter().map(|(_, t)| t.as_str()).collect();
  for t in [
    ecad_chrome::NET_CLASS,
    "0.1 inch",
    ecad_chrome::CMDLINE_HINT,
    "BROWSER",
    "Schematic",
    "Document Settings",
    "Named Views",
    "Sheet 1/1",
    "PLACE COMPONENTS",
    "Left-click diagonal drag to select objects",
    "SHEETS",
    "TEXT COMMANDS",
    "COMMENTS",
  ] {
    assert!(shown.contains(&t), "{t} shows: {shown:?}");
  }
  let readout = |s: &str| {
    s.strip_suffix(" mm")
      .and_then(|xy| xy.split_once(", "))
      .is_some_and(|(x, y)| x.parse::<f64>().is_ok() && y.parse::<f64>().is_ok())
  };
  assert!(!shown.iter().any(|s| readout(s)), "no x, y mm readout: {shown:?}");
  // the browser at the left of the sheet, PLACE COMPONENTS at its right, the bars at the bottom
  let at = |t: &str| all.iter().find(|(_, s)| s == t).unwrap().0;
  let canvas = h.app.canvas;
  assert!(at("BROWSER").left() < canvas.left() + 60.0);
  assert!(at("PLACE COMPONENTS").left() > canvas.right());
  assert!(at("SHEETS").top() > canvas.bottom() && at("COMMENTS").top() > canvas.bottom());
  assert!(at(ecad_chrome::NET_CLASS).bottom() < canvas.top(), "the command row is under the ribbon");
  // the grid readout follows the cursor in inches
  h.frame(vec![Event::PointerMoved(canvas.center())]);
  let shown = h.texts();
  assert!(shown.iter().any(|s| s.starts_with("0.1 inch (")), "{shown:?}");
  // picking a part in PLACE COMPONENTS places it
  let part = h.app.s.execute("ecad.library", &json!({})).unwrap()[0]["device"].as_str().unwrap().to_string();
  let r = h.find(&part).or_else(|| {
    let all = h.frame(vec![]);
    all.into_iter().find(|(_, t)| t.starts_with(&format!("{part}\n"))).map(|(r, _)| r)
  });
  h.click(r.expect("the part is listed").center());
  assert_eq!(h.app.electronics.as_ref().unwrap().tool, Tool::Place(part.clone()));
  h.click(canvas.center());
  assert_eq!(h.app.s.design.electronics.as_ref().map(|p| p.schematic.parts.len()), Some(1));
  assert_eq!(h.app.electronics.as_ref().unwrap().recent, [part]);
}

#[test]
fn the_command_line_and_text_commands() {
  let mut h = Harness::new();
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  h.frame(vec![]);
  // / activates the command line
  h.frame(vec![Event::PointerMoved(h.app.canvas.center())]);
  let slash = |pressed| Event::Key {
    key: egui::Key::Slash,
    physical_key: None,
    pressed,
    repeat: false,
    modifiers: Default::default(),
  };
  h.frame(vec![slash(true), slash(false)]);
  h.frame(vec![]);
  assert!(h.ctx.memory(|m| m.has_focus(ecad_chrome::cmdline_id())), "/ focuses the command line");
  let mut v = h.app.electronics.take().unwrap();
  ecad_chrome::command_line(&mut h.app, &mut v, "net");
  assert_eq!(v.tool, Tool::Wire);
  ecad_chrome::command_line(&mut h.app, &mut v, "/LABEL VCC");
  assert_eq!((v.tool.clone(), v.label.as_str()), (Tool::Label, "VCC"));
  let part = h.app.s.execute("ecad.library", &json!({})).unwrap()[0]["device"].as_str().unwrap().to_string();
  ecad_chrome::command_line(&mut h.app, &mut v, &format!("ADD {part}"));
  assert_eq!(v.tool, Tool::Place(part.clone()));
  ecad_chrome::command_line(&mut h.app, &mut v, "board");
  assert_eq!(v.sheet, Sheet::Board);
  ecad_chrome::command_line(&mut h.app, &mut v, "frobnicate");
  assert!(h.app.status.as_ref().is_some_and(|s| s.0.contains("FROBNICATE")));
  // TEXT COMMANDS runs FissionOxide's commands in Txt
  ecad_chrome::text_command(&mut h.app, &mut v, &format!(r#"ecad.part.add {{"device": "{part}", "at": [0, 0]}}"#));
  assert_eq!(h.app.s.design.electronics.as_ref().map(|p| p.schematic.parts.len()), Some(1));
  assert!(v.text_log[1].contains("refdes"), "{:?}", v.text_log);
  v.text_mode = ecad_chrome::TextMode::Py;
  ecad_chrome::text_command(&mut h.app, &mut v, "print(1)");
  assert!(v.text_log.last().unwrap().contains("Python"));
  h.app.electronics = Some(v);
}

/// `ui:electronics:new-document-tab`: with Untitled and Untitled(1)..(3) designs open, New
/// Electronics Design is still "Untitled"; a second one beside it is "Untitled(1)".
#[test]
fn electronics_documents_are_numbered_apart_from_designs() {
  let mut h = Harness::new();
  for _ in 0..3 {
    documents::new_document(&mut h.app, fox_engine::model::DesignType::Part, None);
  }
  assert_eq!(documents::tab_label(&h.app.s), "Untitled(3)");
  ecad_chrome::new_electronics_design(&mut h.app, None);
  assert_eq!(h.app.docs.count(), 5);
  assert_eq!(documents::tab_label(&h.app.s), "Untitled");
  ecad_chrome::new_electronics_design(&mut h.app, None);
  assert_eq!(documents::tab_label(&h.app.s), "Untitled(1)");
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  assert_eq!(documents::tab_label(&h.app.s), "Untitled(2)");
  // and Ctrl+N's design after them numbers among the designs: Untitled(4)
  documents::new_document(&mut h.app, fox_engine::model::DesignType::Part, None);
  assert_eq!(documents::tab_label(&h.app.s), "Untitled(4)");
}

/// `ui:electronics:schematic-close-prompt`: an untouched new schematic closes without asking;
/// one with a change asks.
#[test]
fn an_untouched_schematic_closes_without_asking() {
  let mut h = Harness::new();
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  assert_eq!(h.app.docs.count(), 2);
  documents::request_close(&mut h.app, 1);
  assert_eq!(h.app.docs.closing, None, "no prompt");
  assert_eq!(h.app.docs.count(), 1, "the tab closed");
  let shown = h.texts();
  assert!(!shown.iter().any(|t| t.starts_with("Save changes to")), "{shown:?}");
  // a board likewise
  ecad_chrome::new_editor(&mut h.app, Sheet::Board);
  documents::request_close(&mut h.app, 1);
  assert_eq!((h.app.docs.closing, h.app.docs.count()), (None, 1));
  // a schematic with changes asks
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  h.app.s.dirty = true;
  documents::request_close(&mut h.app, 1);
  assert_eq!(h.app.docs.closing, Some(1));
  let shown = h.texts();
  assert!(shown.iter().any(|t| t.starts_with("Save changes to Untitled")), "{shown:?}");
  // and quitting with an untouched schematic open doesn't stop on it
  documents::close_now(&mut h.app, 1);
  ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
  documents::switch_to(&mut h.app, 0);
  assert_eq!(documents::first_asking(&h.app), None);
}
