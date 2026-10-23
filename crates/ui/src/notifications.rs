//! The bell's Notifications panel, as Fusion's: docked under the bell, one card per notice
//! (title, body, a link, its age and an x), and Dismiss all. FissionOxide's notices are the
//! software update (when one is pending) and whatever the app posts with [`post`].

use egui::{Color32, RichText, Stroke, vec2};
use fox_desktop::update::Status;

use crate::{App, theme};

/// Where a notice's link goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
  /// The Software Update window.
  SoftwareUpdate,
  /// Help ▸ What's New (this version's release notes).
  WhatsNew,
  /// A web page.
  Url(String),
}

/// One card of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
  /// Stable key, so a dismissed notice stays dismissed.
  pub id: String,
  pub title: String,
  pub body: String,
  /// The link's text and target.
  pub link: Option<(String, Link)>,
  /// How long ago, as the card shows it ("just now", "2 days ago").
  pub age: String,
}

#[derive(Debug)]
pub struct Notifications {
  /// The panel is open.
  pub open: bool,
  /// Notices the app posted (newest last).
  pub posted: Vec<Notice>,
  /// Ids of dismissed notices.
  dismissed: Vec<String>,
  /// The bell's rectangle, which the panel hangs under.
  pub(crate) anchor: egui::Rect,
}

impl Default for Notifications {
  fn default() -> Self {
    Notifications { open: false, posted: Vec::new(), dismissed: Vec::new(), anchor: egui::Rect::NOTHING }
  }
}

/// The notice a fresh start shows, as Fusion's "What's new" one: this version's release notes.
pub fn whats_new_notice() -> Notice {
  Notice {
    id: format!("whats-new-{}", fox_desktop::VERSION),
    title: format!("What's new in FissionOxide {}", fox_desktop::VERSION),
    body: "The release notes for this version.".into(),
    link: Some(("What's New".into(), Link::WhatsNew)),
    age: "just now".into(),
  }
}

/// Posts a notice (it shows until dismissed).
pub fn post(app: &mut App, notice: Notice) {
  app.notifications.posted.retain(|n| n.id != notice.id);
  app.notifications.posted.push(notice);
}

/// The cards the panel shows now: the pending software update, then the posted notices, newest
/// first, less the dismissed ones.
pub fn cards(app: &App) -> Vec<Notice> {
  let mut out = Vec::new();
  let status = app.updates.status();
  if let Some(release) = status.release() {
    let (title, body) = match &status {
      Status::Ready { .. } => (
        format!("FissionOxide {} is ready to install", release.version),
        "It installs when FissionOxide closes.".into(),
      ),
      Status::Downloading { .. } => (format!("Downloading FissionOxide {}", release.version), String::new()),
      _ => (format!("FissionOxide {} is available", release.version), first_line(&release.notes)),
    };
    out.push(Notice {
      id: format!("update-{}", release.version),
      title,
      body,
      link: Some(("Open Software Update".into(), Link::SoftwareUpdate)),
      age: "just now".into(),
    });
  }
  out.extend(app.notifications.posted.iter().rev().cloned());
  out.retain(|n| !app.notifications.dismissed.contains(&n.id));
  out
}

/// The bell shows its unread dot while there is something to read.
pub fn unread(app: &App) -> bool {
  !cards(app).is_empty()
}

pub fn toggle(app: &mut App) {
  app.notifications.open = !app.notifications.open;
}

pub fn dismiss(app: &mut App, id: &str) {
  app.notifications.dismissed.push(id.to_string());
}

pub fn dismiss_all(app: &mut App) {
  for n in cards(app) {
    app.notifications.dismissed.push(n.id);
  }
}

fn first_line(notes: &str) -> String {
  notes.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or("").to_string()
}

/// The panel, while open. A click anywhere else closes it, as a menu would.
pub fn panel(app: &mut App, ctx: &egui::Context) {
  if !app.notifications.open {
    return;
  }
  let anchor = app.notifications.anchor;
  let cards = cards(app);
  let mut dismiss_ids: Vec<String> = Vec::new();
  let mut dismiss_everything = false;
  let mut follow: Option<Link> = None;
  let r = egui::Area::new(egui::Id::new("notifications"))
    .fixed_pos(anchor.right_bottom() + vec2(0.0, 6.0))
    .pivot(egui::Align2::RIGHT_TOP)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(3.0)
        .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(40) })
        .inner_margin(10.0)
        .show(ui, |ui| {
          ui.set_width(300.0);
          ui.horizontal(|ui| {
            ui.label(RichText::new("Notifications").strong().size(14.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if ui.add_enabled(!cards.is_empty(), egui::Button::new("Dismiss all").frame(false)).clicked() {
                dismiss_everything = true;
              }
            });
          });
          ui.separator();
          if cards.is_empty() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("No notifications").color(theme::MUTED)));
            ui.add_space(12.0);
          }
          for n in &cards {
            egui::Frame::new()
              .fill(theme::DIALOG_BODY)
              .stroke(Stroke::new(1.0, theme::BORDER))
              .corner_radius(2.0)
              .inner_margin(8.0)
              .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                  ui.label(RichText::new(&n.title).strong());
                  ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("🗙").frame(false)).on_hover_text("Dismiss").clicked() {
                      dismiss_ids.push(n.id.clone());
                    }
                  });
                });
                if !n.body.is_empty() {
                  ui.label(&n.body);
                }
                ui.horizontal(|ui| {
                  if let Some((text, link)) = &n.link
                    && ui.link(text).clicked()
                  {
                    follow = Some(link.clone());
                  }
                  ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&n.age).small().color(theme::MUTED));
                  });
                });
              });
            ui.add_space(4.0);
          }
        });
    });
  for id in dismiss_ids {
    dismiss(app, &id);
  }
  if dismiss_everything {
    dismiss_all(app);
  }
  match follow {
    Some(Link::SoftwareUpdate) => {
      app.updates.open = true;
      app.notifications.open = false;
    }
    Some(Link::WhatsNew) => {
      app.notifications.open = false;
      crate::help::whats_new(app);
    }
    Some(Link::Url(u)) => {
      let _ = fox_desktop::open(&u);
    }
    None => {}
  }
  // a click outside the panel and the bell closes it
  let clicked_outside = ctx.input(|i| {
    i.pointer.any_click()
      && i.pointer.interact_pos().is_some_and(|p| !r.response.rect.contains(p) && !anchor.contains(p))
  });
  if clicked_outside {
    app.notifications.open = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn notice(id: &str) -> Notice {
    Notice { id: id.into(), title: id.to_uppercase(), body: "body".into(), link: None, age: "just now".into() }
  }

  #[test]
  fn the_bell_lists_posted_notices_until_dismissed() {
    let mut app = App::with_session(Session::default());
    assert!(cards(&app).is_empty(), "no updater, nothing posted");
    assert!(!unread(&app));
    post(&mut app, notice("a"));
    post(&mut app, notice("b"));
    assert_eq!(cards(&app).iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["b", "a"], "newest first");
    assert!(unread(&app));
    // posting the same id again replaces it, keeping one card
    post(&mut app, notice("a"));
    assert_eq!(cards(&app).len(), 2);
    dismiss(&mut app, "a");
    assert_eq!(cards(&app).len(), 1);
    dismiss_all(&mut app);
    assert!(cards(&app).is_empty());
    assert!(!unread(&app), "the dot goes with the last card");
    // the bell toggles the panel
    toggle(&mut app);
    assert!(app.notifications.open);
    toggle(&mut app);
    assert!(!app.notifications.open);
  }

  #[test]
  fn the_panel_draws_its_cards() {
    let mut app = App::with_session(Session::default());
    post(&mut app, notice("a"));
    app.notifications.open = true;
    let ctx = egui::Context::default();
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0))),
      ..Default::default()
    };
    ctx.run_ui(input, |ui| panel(&mut app, ui.ctx())).drop_without_applying_deltas();
    assert!(app.notifications.open, "nothing was clicked");
    let r = ctx.memory(|m| m.area_rect(egui::Id::new("notifications"))).expect("the panel is drawn");
    assert!(r.width() >= 300.0, "{r:?}");
  }
}
