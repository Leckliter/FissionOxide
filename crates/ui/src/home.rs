//! The Home view, as Fusion's: the application bar's house shows it in place of the design (the
//! window titled "Home - FissionOxide"), and a click on a document's tab goes back.
//!
//! A left column (the Home chip, FissionOxide, the hub, Open... and New..., My FissionOxide /
//! Projects / Samples) and a page: a search bar for the hub, "Hi <name>", two cards, Start
//! working (New Design, New Part Design, New Project, Open, Upload), Recent (list or grid, a
//! search field, filter and settings, a Name / Location / Owner table) and Ways to explore.
//! What it lists comes from the hub's `hub.recent`, `hub.search` and `hub.info`.

use egui::{Color32, RichText, Stroke, vec2};
use fox_engine::model::DesignType;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::{App, logo, theme};

/// How often Recent is re-read while the view shows (seconds).
const REFRESH_SECS: f64 = 5.0;

/// The left column's width.
const COLUMN_W: f32 = 230.0;

/// The search bar's hint, as Fusion's.
pub const SEARCH_HINT: &str = "Search by design properties or file name in current hub";

/// Start working's buttons, in Fusion's order (the first is the blue one).
pub const START: [&str; 5] = ["New Design", "New Part Design", "New Project", "Open", "Upload"];

/// Recent's columns.
pub const COLUMNS: [&str; 3] = ["Name", "Location", "Owner"];

/// Why Samples is greyed.
const NO_SAMPLES: &str = "Fusion's samples are Autodesk cloud content, which FissionOxide doesn't have";

/// Recent's filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Show {
  #[default]
  All,
  Designs,
  Files,
}

impl Show {
  const ALL: [Show; 3] = [Show::All, Show::Designs, Show::Files];

  fn label(self) -> &'static str {
    match self {
      Show::All => "All",
      Show::Designs => "Designs",
      Show::Files => "Other files",
    }
  }

  fn keeps(self, item: &Value) -> bool {
    let design = item.get("kind").and_then(Value::as_str) == Some("design");
    match self {
      Show::All => true,
      Show::Designs => design,
      Show::Files => !design,
    }
  }
}

#[derive(Default)]
pub struct Home {
  /// The view shows in place of the design.
  pub open: bool,
  /// The search bar's text, and the search last run with what it found.
  pub search: String,
  pub(crate) results: Option<(String, Vec<Value>)>,
  /// Recent's own search field, filter and sort (by name, else newest first).
  pub filter: String,
  pub show: Show,
  pub by_name: bool,
  /// Recent as a grid of tiles (otherwise a table, Fusion's default).
  pub grid: bool,
  /// The row a click picked.
  pub selected: Option<String>,
  /// `hub.recent` and who we are on the hub, and when they were read.
  pub(crate) recent: Vec<Value>,
  pub(crate) owner: String,
  read_at: Option<f64>,
}

/// The house: shows the Home view (its lists read afresh).
pub fn open(app: &mut App) {
  app.home.open = true;
  app.home.read_at = None;
}

/// "Hi Collin": the greeting's first name, from the user's name.
pub fn greeting(user: &str) -> String {
  let Some(first) = user.split(['.', '_', ' ', '-']).find(|p| !p.is_empty()) else { return "Hi there".into() };
  let mut c = first.chars();
  let name: String = c.next().into_iter().flat_map(char::to_uppercase).chain(c).collect();
  format!("Hi {name}")
}

fn s(v: &Value, k: &str) -> String {
  v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Where an item is: its project and folders.
pub fn location(item: &Value) -> String {
  let mut parts = vec![s(item, "project_name")];
  parts.extend(
    item.get("path").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from),
  );
  parts.retain(|p| !p.is_empty());
  parts.join(" / ")
}

/// The rows Recent shows: the search's results while one is run, else the recent items, less
/// what the filter and the search field leave out.
pub fn rows(home: &Home) -> Vec<Value> {
  let base = home.results.as_ref().map_or(&home.recent, |(_, r)| r);
  let t = home.filter.trim().to_lowercase();
  let mut out: Vec<Value> = base
    .iter()
    .filter(|i| home.show.keeps(i) && (t.is_empty() || s(i, "name").to_lowercase().contains(&t)))
    .cloned()
    .collect();
  if home.by_name {
    out.sort_by_key(|i| s(i, "name").to_lowercase());
  }
  out
}

fn refresh(app: &mut App, now: f64) {
  if app.home.read_at.is_some_and(|t| now - t < REFRESH_SECS) {
    return;
  }
  app.home.read_at = Some(now);
  let recent = app.s.execute("hub.recent", &json!({"limit": 50})).ok();
  app.home.recent = recent.and_then(|v| v.as_array().cloned()).unwrap_or_default();
  let info = app.s.execute("hub.info", &json!({})).unwrap_or(Value::Null);
  app.home.owner = info.get("user").and_then(Value::as_str).map(String::from).unwrap_or_else(crate::panels::user_name);
}

/// Runs the search bar's search.
pub fn search(app: &mut App) {
  let q = app.home.search.trim().to_string();
  if q.is_empty() {
    app.home.results = None;
    return;
  }
  let found = match app.s.execute("hub.search", &json!({"query": q})) {
    Ok(v) => v.as_array().cloned().unwrap_or_default(),
    Err(e) => {
      app.status = Some((e.0, true));
      Vec::new()
    }
  };
  app.home.results = Some((q, found));
}

/// What a click on the page asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
  Start(&'static str),
  New,
  OpenItem(Value),
  Projects,
  WhatsNew,
  Docs,
  Forum,
  Feedback,
  Releases,
  Search,
}

/// Does what the page asked. Everything that opens a design leaves the Home view for it.
pub fn act(app: &mut App, a: Act) {
  match a {
    Act::Search => return search(app),
    Act::WhatsNew => return crate::help::whats_new(app),
    Act::Docs => return crate::help::open(app, crate::help::DOCS),
    Act::Forum => return crate::help::action(app, "Forum"),
    Act::Feedback => return crate::panels::profile_action(app, "Share Feedback"),
    Act::Releases => {
      let _ = fox_desktop::open(fox_desktop::update::RELEASES_PAGE);
      return;
    }
    _ => {}
  }
  app.home.open = false;
  match a {
    Act::Start("New Design") => crate::documents::new_document(app, DesignType::Hybrid, None),
    Act::Start("New Part Design") => crate::documents::new_document(app, DesignType::Part, None),
    Act::Start("New Project") => {
      app.data.open = true;
      crate::data_panel::new_project(app);
    }
    Act::Start("Open") => app.open_dialog(),
    Act::Start("Upload") => crate::data_panel::upload_dialog(app),
    Act::New => crate::documents::open_chooser(app),
    Act::OpenItem(it) => crate::data_panel::item_action(app, "Open", &it),
    Act::Projects => {
      app.data.open = true;
      app.data.invalidate();
    }
    _ => {}
  }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  refresh(app, ui.input(|i| i.time));
  let full = ui.max_rect();
  ui.painter().rect_filled(full, 0.0, theme::PANEL);
  let column = egui::Rect::from_min_max(full.min, egui::pos2(full.left() + COLUMN_W, full.bottom()));
  let page = egui::Rect::from_min_max(egui::pos2(column.right(), full.top()), full.max);
  ui.painter().rect_filled(column, 0.0, Color32::WHITE);
  ui.painter().vline(column.right(), column.y_range(), Stroke::new(1.0, theme::BORDER));
  let mut todo: Option<Act> = None;
  ui.scope_builder(egui::UiBuilder::new().max_rect(column.shrink(12.0)).id_salt("home-column"), |ui| {
    left_column(app, ui, &mut todo);
  });
  ui.scope_builder(egui::UiBuilder::new().max_rect(page).id_salt("home-page"), |ui| {
    egui::ScrollArea::vertical().id_salt("home-scroll").auto_shrink([false, false]).show(ui, |ui| {
      let w = ui.available_width().min(1000.0);
      let margin = ((ui.available_width() - w) / 2.0).max(0.0) + 24.0;
      egui::Frame::NONE.inner_margin(egui::Margin { left: margin as i8, right: 24, top: 18, bottom: 24 }).show(
        ui,
        |ui| {
          ui.set_width(w - 48.0);
          page_body(app, ui, &mut todo);
        },
      );
    });
  });
  if let Some(a) = todo {
    act(app, a);
  }
}

fn left_column(app: &mut App, ui: &mut egui::Ui, todo: &mut Option<Act>) {
  ui.spacing_mut().item_spacing.y = 8.0;
  // the Home chip, picked
  let (chip, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), egui::Sense::hover());
  ui.painter().rect_filled(chip, 15.0, icons::tint_of(theme::ACCENT, 0.85));
  icons::draw(
    ui.painter(),
    egui::Rect::from_center_size(chip.left_center() + vec2(20.0, 0.0), vec2(16.0, 16.0)),
    Icon::AppHome,
    theme::ACCENT,
  );
  ui.painter().text(
    chip.left_center() + vec2(36.0, 0.0),
    egui::Align2::LEFT_CENTER,
    "Home",
    egui::FontId::proportional(13.5),
    theme::ACCENT,
  );
  ui.add_space(4.0);
  ui.horizontal(|ui| {
    let (r, _) = ui.allocate_exact_size(vec2(22.0, 24.0), egui::Sense::hover());
    logo::paint(ui.painter(), r);
    ui.label(RichText::new("FissionOxide").strong().size(16.0));
  });
  // the hub, as Fusion's "Collin Leckliter v"
  let owner = app.home.owner.clone();
  ui.menu_button(RichText::new(format!("{owner} ⏷")).color(theme::TEXT), |ui| {
    ui.label(RichText::new(&owner).strong());
    ui.label(RichText::new("FissionOxide Hub").color(theme::MUTED));
  });
  ui.horizontal(|ui| {
    let w = (ui.available_width() - 8.0) / 2.0;
    if ui.add(egui::Button::new("Open...").min_size(vec2(w, 28.0))).clicked() {
      *todo = Some(Act::Start("Open"));
    }
    if ui.add(egui::Button::new("New...").min_size(vec2(w, 28.0))).clicked() {
      *todo = Some(Act::New);
    }
  });
  ui.add_space(4.0);
  let nav = |ui: &mut egui::Ui, label: &str, picked: bool, enabled: bool| {
    let text = RichText::new(label).color(if picked { theme::ACCENT } else { theme::TEXT });
    ui.add_enabled(
      enabled,
      egui::Button::new(text).frame(false).selected(picked).min_size(vec2(ui.available_width(), 26.0)),
    )
  };
  nav(ui, "My FissionOxide", true, true);
  if nav(ui, "Projects", false, true).on_hover_text("The projects in the Data Panel").clicked() {
    *todo = Some(Act::Projects);
  }
  nav(ui, "Samples", false, false).on_disabled_hover_text(NO_SAMPLES);
}

/// A white card with a border.
fn card(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
  ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
    egui::Frame::NONE
      .fill(Color32::WHITE)
      .stroke(Stroke::new(1.0, theme::BORDER))
      .corner_radius(6.0)
      .inner_margin(14.0)
      .show(ui, |ui| {
        ui.set_width(w - 28.0);
        add(ui);
      });
  })
  .response
  .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn heading(ui: &mut egui::Ui, text: &str) {
  ui.add_space(14.0);
  ui.label(RichText::new(text).strong().size(17.0));
  ui.add_space(4.0);
}

fn page_body(app: &mut App, ui: &mut egui::Ui, todo: &mut Option<Act>) {
  ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
  // the search bar
  ui.horizontal(|ui| {
    let field = egui::TextEdit::singleline(&mut app.home.search)
      .hint_text(SEARCH_HINT)
      .desired_width(ui.available_width() - 90.0)
      .margin(vec2(10.0, 7.0));
    let r = ui.add(field);
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let b =
      egui::Button::new(RichText::new("Search").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(80.0, 30.0));
    if ui.add(b).clicked() || enter {
      *todo = Some(Act::Search);
    }
  });
  ui.add_space(10.0);
  ui.horizontal(|ui| {
    ui.label(RichText::new(greeting(&crate::panels::user_name())).strong().size(24.0));
    // the pencil edits the profile in Fusion; FissionOxide has no account to edit
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
    icons::draw(ui.painter(), r, Icon::Pencil, theme::MUTED);
    resp.on_hover_text(crate::panels::NO_ACCOUNT);
  });
  // the two cards
  ui.horizontal_top(|ui| {
    let w = (ui.available_width() - 8.0) / 2.0;
    let whats_new = card(ui, w, |ui| {
      ui.label(RichText::new(format!("What's new in FissionOxide {}", fox_desktop::VERSION)).strong().size(14.5));
      ui.label(RichText::new("See what this version brings.").color(theme::MUTED));
    });
    if whats_new.clicked() {
      *todo = Some(Act::WhatsNew);
    }
    let feedback = card(ui, w, |ui| {
      ui.label(RichText::new("Tell us what you think").strong().size(14.5));
      ui.label(RichText::new("Share feedback and ideas for FissionOxide.").color(theme::MUTED));
    });
    if feedback.clicked() {
      *todo = Some(Act::Feedback);
    }
  });
  heading(ui, "Start working");
  ui.horizontal_wrapped(|ui| {
    for (i, label) in START.into_iter().enumerate() {
      let b = if i == 0 {
        egui::Button::new(RichText::new(format!("+ {label}")).color(Color32::WHITE).strong()).fill(theme::ACCENT)
      } else {
        egui::Button::new(label).fill(Color32::WHITE)
      };
      if ui.add(b.min_size(vec2(130.0, 34.0)).corner_radius(4)).clicked() {
        *todo = Some(Act::Start(label));
      }
    }
  });
  recent(app, ui, todo);
  heading(ui, "Ways to explore");
  ui.horizontal_top(|ui| {
    let w = (ui.available_width() - 16.0) / 3.0;
    let ways = [
      ("Learning and Documentation", "Guides to every workspace.", Act::Docs),
      ("Forum", "Questions, ideas and answers from other users.", Act::Forum),
      ("Releases", "Every version and its release notes.", Act::Releases),
    ];
    for (title, body, a) in ways {
      let r = card(ui, w, |ui| {
        ui.label(RichText::new(title).strong());
        ui.label(RichText::new(body).color(theme::MUTED));
      });
      if r.clicked() {
        *todo = Some(a);
      }
    }
  });
}

/// The Recent section: its bar (list / grid, a search field, filter and settings) and the table
/// or the tiles.
fn recent(app: &mut App, ui: &mut egui::Ui, todo: &mut Option<Act>) {
  ui.add_space(14.0);
  ui.horizontal(|ui| {
    let title = match &app.home.results {
      Some((q, _)) => format!("Search results for \"{q}\""),
      None => "Recent".to_string(),
    };
    ui.label(RichText::new(title).strong().size(17.0));
    if app.home.results.is_some() && ui.link("Clear").clicked() {
      app.home.results = None;
      app.home.search.clear();
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      ui.menu_button("⚙", |ui| {
        ui.label(RichText::new("Sort by").strong());
        ui.radio_value(&mut app.home.by_name, false, "Last updated");
        ui.radio_value(&mut app.home.by_name, true, "Name");
      })
      .response
      .on_hover_text("Settings");
      ui.menu_button("⏷", |ui| {
        for f in Show::ALL {
          if ui.radio_value(&mut app.home.show, f, f.label()).clicked() {
            ui.close();
          }
        }
      })
      .response
      .on_hover_text("Filter");
      ui.add(egui::TextEdit::singleline(&mut app.home.filter).hint_text("Search").desired_width(160.0));
      if ui.selectable_label(app.home.grid, "⊞").on_hover_text("Grid view").clicked() {
        app.home.grid = true;
      }
      if ui.selectable_label(!app.home.grid, "☰").on_hover_text("List view").clicked() {
        app.home.grid = false;
      }
    });
  });
  let rows = rows(&app.home);
  let owner = app.home.owner.clone();
  if app.home.grid {
    ui.horizontal_wrapped(|ui| {
      for it in &rows {
        let id = s(it, "id");
        let picked = app.home.selected.as_deref() == Some(id.as_str());
        let r = ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
          let stroke = if picked { Stroke::new(1.5, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
          egui::Frame::NONE.fill(Color32::WHITE).stroke(stroke).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(140.0);
            ui.vertical_centered(|ui| {
              let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), egui::Sense::hover());
              item_icon(ui, r, it);
              ui.add(egui::Label::new(s(it, "name")).truncate());
            });
          });
        });
        pick(app, &r.response, &id, it, todo);
      }
    });
  } else {
    egui::Frame::NONE.fill(Color32::WHITE).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(4.0).show(ui, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      let width = ui.available_width();
      let cols = [12.0, width * 0.45, width * 0.78];
      let (head, _) = ui.allocate_exact_size(vec2(width, 30.0), egui::Sense::hover());
      for (c, x) in COLUMNS.iter().zip(cols) {
        ui.painter().text(
          egui::pos2(head.left() + x, head.center().y),
          egui::Align2::LEFT_CENTER,
          *c,
          egui::FontId::proportional(12.5),
          theme::GROUP_LABEL,
        );
      }
      ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, theme::BORDER));
      for it in &rows {
        let id = s(it, "id");
        let (rect, r) = crate::file_dialogs::row(ui, app.home.selected.as_deref() == Some(id.as_str()));
        let ir = egui::Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), vec2(16.0, 16.0));
        item_icon(ui, ir, it);
        crate::file_dialogs::row_text(ui, rect, cols[0] + 22.0, &s(it, "name"), theme::TEXT);
        crate::file_dialogs::row_text(ui, rect, cols[1], &location(it), theme::MUTED);
        crate::file_dialogs::row_text(ui, rect, cols[2], &owner, theme::MUTED);
        pick(app, &r, &id, it, todo);
      }
      if rows.is_empty() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
          let text =
            if app.home.results.is_some() { "Nothing matches." } else { "Designs you open or save show here." };
          ui.label(RichText::new(text).color(theme::MUTED));
        });
        ui.add_space(16.0);
      }
    });
  }
}

/// A click picks a row; a double-click opens it.
fn pick(app: &mut App, r: &egui::Response, id: &str, it: &Value, todo: &mut Option<Act>) {
  if r.double_clicked() {
    *todo = Some(Act::OpenItem(it.clone()));
  } else if r.clicked() {
    app.home.selected = Some(id.to_string());
  }
}

fn item_icon(ui: &egui::Ui, r: egui::Rect, it: &Value) {
  if s(it, "kind") == "design" {
    icons::draw_color(ui.painter(), r, Icon::Component);
  } else {
    icons::draw(ui.painter(), r, Icon::File, theme::ICON_INK);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_greeting_and_locations_read_as_fusions() {
    assert_eq!(greeting("collin"), "Hi Collin");
    assert_eq!(greeting("collin.leckliter"), "Hi Collin");
    assert_eq!(greeting(""), "Hi there");
    let it = json!({"name": "Bracket", "project_name": "Default Project", "path": ["Parts", "Brackets"]});
    assert_eq!(location(&it), "Default Project / Parts / Brackets");
  }

  #[test]
  fn recent_filters_searches_and_sorts() {
    let mut home = Home {
      recent: vec![
        json!({"id": "1", "name": "Zeta", "kind": "design"}),
        json!({"id": "2", "name": "alpha.png", "kind": "file"}),
        json!({"id": "3", "name": "Beta", "kind": "design"}),
      ],
      ..Home::default()
    };
    let names = |h: &Home| rows(h).iter().map(|i| s(i, "name")).collect::<Vec<_>>();
    assert_eq!(names(&home), ["Zeta", "alpha.png", "Beta"], "newest first, as the hub gives them");
    home.by_name = true;
    assert_eq!(names(&home), ["alpha.png", "Beta", "Zeta"]);
    home.show = Show::Designs;
    assert_eq!(names(&home), ["Beta", "Zeta"]);
    home.filter = "ze".into();
    assert_eq!(names(&home), ["Zeta"]);
    home.filter.clear();
    home.show = Show::All;
    home.results = Some(("al".into(), vec![json!({"id": "2", "name": "alpha.png", "kind": "file"})]));
    assert_eq!(names(&home), ["alpha.png"], "a search shows its results");
  }
}
