//! MANAGE ▸ Bill of Materials' palette (`ui:manage:bom-palette`) and its toolbar, dropdowns and
//! Column Settings (`ui:manage:bom-toolbar-menus`), driven without a screen.

use egui::{Event, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;
use serde_json::json;

use crate::App;
use crate::bom_ui::{self, View};
use crate::dialogs::Kind;
use crate::panels::Cmd;

/// The prober's window: 908 px wide.
struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
}

impl Harness {
  fn new(app: App) -> Harness {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness { ctx, app, time: 0.0 };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  fn frame(&mut self, events: Vec<Event>) {
    self.time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(908.0, 860.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    self.ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
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

  fn area(&self, id: &str) -> Option<Rect> {
    self.ctx.memory(|m| m.area_rect(egui::Id::new(id)))
  }

  fn palette(&self) -> Rect {
    self.area("bom-palette").expect("the palette is drawn")
  }

  fn menu_open(&self, id: &str) -> bool {
    egui::Popup::is_id_open(&self.ctx, egui::Id::new(id))
  }
}

/// Two parts in components (one Aluminum, with a part number and description) and the root.
fn assembly() -> App {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
  app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10, "at": [50, 50]})).unwrap();
  let (a, b) = (app.s.model.bodies[0].id.0, app.s.model.bodies[1].id.0);
  app
    .s
    .execute(
      "component.create",
      &json!({"name": "Bracket", "bodies": [a], "part_number": "BR-01", "description": "Mount, left"}),
    )
    .unwrap();
  app.s.execute("component.create", &json!({"name": "Pin", "bodies": [b]})).unwrap();
  let pin = app.s.design.components.iter().find(|c| c.name == "Pin").unwrap().id.0;
  app.s.execute("component.material", &json!({"component": pin, "material": "Aluminum"})).unwrap();
  app
}

/// Saved once: Fusion shows BOM data only then.
fn saved(mut app: App) -> App {
  app.s.path = Some(std::env::temp_dir().join("bom-test.fox"));
  app
}

#[test]
fn bill_of_materials_opens_a_docked_palette_not_a_dialog() {
  let mut h = Harness::new(App::with_session(Session::default()));
  Cmd::Dialog(Kind::Bom).run(&mut h.app);
  assert!(h.app.bom.is_some(), "the palette opens");
  assert!(h.app.dialog.is_none(), "no command dialog");
  assert!(!Cmd::Dialog(Kind::Bom).active(&h.app), "the ribbon button doesn't stay pressed");
  h.frame(vec![]);
  h.frame(vec![]);
  // about 450 x 370, over the canvas's lower right
  let r = h.palette();
  assert!((448.0..=456.0).contains(&r.width()) && (366.0..=376.0).contains(&r.height()), "{r:?}");
  let canvas = h.app.canvas;
  assert!((r.right() - canvas.right()).abs() < 16.0 && (r.bottom() - canvas.bottom()).abs() < 16.0, "{r:?} {canvas:?}");
  // never saved: the empty state rather than the table
  assert!(!bom_ui::has_data(&h.app, h.app.bom.as_ref().unwrap()));

  // the "-" folds it to its header and back
  let fold = r.left_top() + vec2(16.0, 16.0);
  h.click(fold);
  assert!(h.app.bom.as_ref().unwrap().folded);
  assert!(h.palette().height() < 40.0, "{:?}", h.palette());
  h.click(h.palette().left_top() + vec2(16.0, 16.0));
  assert!(!h.app.bom.as_ref().unwrap().folded);
  h.frame(vec![]);

  // ">>" tucks it into a tab at the canvas's right edge; the tab brings it back
  let r = h.palette();
  h.click(r.right_top() + vec2(-12.0, 16.0));
  assert!(h.app.bom.as_ref().unwrap().tucked);
  let tab = h.area("bom-palette-tab").expect("the tab is drawn");
  assert!((tab.right() - h.app.canvas.right()).abs() < 4.0, "{tab:?}");
  h.click(tab.center());
  assert!(!h.app.bom.as_ref().unwrap().tucked);
  h.frame(vec![]);

  // Close shuts it
  let r = h.palette();
  h.click(r.right_bottom() + vec2(-48.0, -20.0));
  assert!(h.app.bom.is_none(), "Close closes the palette");
}

#[test]
fn the_dropdowns_open_under_their_fields_and_close_on_a_second_click() {
  let mut h = Harness::new(saved(assembly()));
  Cmd::Dialog(Kind::Bom).run(&mut h.app);
  h.frame(vec![]);
  h.frame(vec![]);
  let r = h.palette();
  // the toolbar row under the header: Structured, then Working
  let structured = r.left_top() + vec2(1.0 + 10.0 + 56.0, 1.0 + 30.0 + 18.0);
  let working = structured + vec2(56.0 + 8.0 + 50.0, 0.0);
  for (field, menu) in [(structured, "bom-view-menu"), (working, "bom-version-menu")] {
    h.click(field);
    assert!(h.menu_open(menu), "{menu} opens");
    h.click(field);
    assert!(!h.menu_open(menu), "{menu} closes when its field is clicked again");
  }
  // the gear's Column Settings, then the kebab's menu, at the toolbar's right end
  let kebab = r.right_top() + vec2(-1.0 - 10.0 - 12.0, 1.0 + 30.0 + 18.0);
  let gear = kebab - vec2(28.0, 0.0);
  h.click(gear);
  assert!(h.menu_open("bom-columns"), "Column Settings opens");
  h.click(gear);
  assert!(!h.menu_open("bom-columns"));
  h.click(kebab);
  assert!(h.menu_open("bom-more-menu"), "the kebab menu opens");
  h.click(kebab);
  assert!(!h.menu_open("bom-more-menu"));
}

#[test]
fn the_table_lists_the_components_in_either_view() {
  let mut app = saved(assembly());
  bom_ui::open(&mut app);
  let mut b = app.bom.take().unwrap();
  assert!(bom_ui::has_data(&app, &b));
  assert_eq!(b.view, View::Structured);
  let names: Vec<&str> = b.rows.iter().filter_map(|r| r["name"].as_str()).collect();
  assert_eq!(names, ["Bracket", "Pin"]);
  let bracket = &b.rows[0];
  assert_eq!(bom_ui::cell(&app, bracket, "Part Number"), "BR-01");
  assert_eq!(bom_ui::cell(&app, bracket, "Description"), "Mount, left");
  assert_eq!(bom_ui::cell(&app, bracket, "Quantity"), "1");
  assert_eq!(bom_ui::cell(&app, bracket, "Material"), "Steel");
  assert_eq!(bom_ui::cell(&app, bracket, "Revision/Version"), "v1");
  assert_eq!(bom_ui::cell(&app, bracket, "Lifecycle"), "", "Manage's columns have no data here");
  // no part number: the name, as in Fusion
  assert_eq!(bom_ui::cell(&app, &b.rows[1], "Part Number"), "Pin");
  assert_eq!(bom_ui::cell(&app, &b.rows[1], "Material"), "Aluminum");

  // Parts Only counts alike parts
  bom_ui::set_view(&mut app, &mut b, View::PartsOnly);
  assert_eq!(b.view, View::PartsOnly);
  assert!(b.rows.iter().all(|r| r.get("quantity").is_some()), "{:?}", b.rows);
  assert_eq!(b.checked.len(), b.rows.len());

  // the CSV download: Part Name and the shown columns, quoted where needed
  bom_ui::set_view(&mut app, &mut b, View::Structured);
  let csv = bom_ui::csv(&app, &b);
  let mut lines = csv.lines();
  assert_eq!(
    lines.next(),
    Some(
      "Part Name,Revision/Version,Description,Part Number,Quantity,Material,Item Number,Lifecycle,Revision,State,Change Order"
    )
  );
  assert_eq!(lines.next(), Some("Bracket,v1,\"Mount, left\",BR-01,1,Steel,,,,,"));

  // drawn with data, in both views, and folded
  app.bom = Some(b);
  let mut h = Harness::new(app);
  h.frame(vec![]);
  assert!(h.palette().height() > 360.0);
  let mut b = h.app.bom.take().unwrap();
  bom_ui::set_view(&mut h.app, &mut b, View::PartsOnly);
  h.app.bom = Some(b);
  h.frame(vec![]);
}

#[test]
fn column_settings_tick_columns_and_reorder_them_in_the_preferences() {
  let mut app = App::with_session(Session::default());
  let prefs = &app.preferences.values;
  // GENERAL all ticked but Material (Legacy); MANAGE all ticked
  assert_eq!(prefs.bom_hidden, ["Material (Legacy)"]);
  assert_eq!(
    bom_ui::columns(prefs),
    [
      "Thumbnail",
      "Revision/Version",
      "Description",
      "Part Number",
      "Quantity",
      "Material",
      "Item Number",
      "Lifecycle",
      "Revision",
      "State",
      "Change Order"
    ]
  );
  assert_eq!(bom_ui::GENERAL.len() + bom_ui::MANAGE.len(), bom_ui::ORDER.len());

  bom_ui::set_column(&mut app, "Thumbnail", false);
  bom_ui::set_column(&mut app, "Material (Legacy)", true);
  let cols = bom_ui::columns(&app.preferences.values);
  assert!(!cols.contains(&"Thumbnail") && cols.contains(&"Material (Legacy)"));
  assert_eq!(app.preferences.draft.bom_hidden, app.preferences.values.bom_hidden, "the dialog's OK keeps them");

  // a header dragged onto another's place; Reset Order puts Fusion's back, the ticks stay
  bom_ui::move_column(&mut app, "Material", "Description");
  assert_eq!(bom_ui::columns(&app.preferences.values)[1..4], ["Material", "Description", "Part Number"]);
  bom_ui::reset_order(&mut app);
  assert_eq!(bom_ui::columns(&app.preferences.values)[1], "Description");
  assert!(!bom_ui::columns(&app.preferences.values).contains(&"Thumbnail"));

  // saved with the preferences; every column ticked is written as an empty list
  bom_ui::set_column(&mut app, "Thumbnail", true);
  let ini = app.preferences.values.write_into("");
  let back = crate::preferences::Prefs::parse(&ini);
  assert!(back.bom_hidden.is_empty(), "{ini}");
  assert_eq!(back, app.preferences.values);
  // an unknown name in a saved order is skipped
  app.preferences.values.bom_order = vec!["Weight".into(), "State".into()];
  assert_eq!(bom_ui::columns(&app.preferences.values)[0], "State");
}
