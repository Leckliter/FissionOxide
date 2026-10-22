//! The application bar's Job Status, as Fusion's: Help ▸ Check for Updates opens no window, it
//! reports here. The icon carries a "1" badge while there is news; a click opens the panel under
//! it with the update card (a gift, "A new version of FissionOxide has landed.", Check out what's
//! new, Update Now and Update Later) and, under a separator, the Working online toggle.

use egui::{Color32, RichText, Stroke, vec2};
use fox_desktop::update::{InstallStyle, Status};

use crate::icons::{self, Icon};
use crate::{App, theme};

/// How long Update Later puts an update off, as Fusion's (seconds).
pub const LATER: u64 = 3 * 24 * 60 * 60;

/// The update card's lines, as Fusion's.
pub const LANDED: &str = "A new version of FissionOxide has landed.";
pub const LANDED_BODY: &str = "Click Update Now to begin. Click Update Later to delay the update for 3 days.";

/// The icon's tooltip.
pub const TIP: &str = "Job Status";

#[derive(Debug)]
pub struct JobStatus {
  /// The panel is open.
  pub open: bool,
  /// Working online (Fusion's toggle, on by default). Offline, FissionOxide doesn't look for
  /// updates.
  pub online: bool,
  /// A check's result hasn't been looked at yet.
  pub(crate) unseen: bool,
  /// The release Update Later put off, and until when (seconds since the epoch).
  pub(crate) later: Option<(String, u64)>,
  /// The icon's rectangle, which the panel hangs under.
  pub(crate) anchor: egui::Rect,
}

impl Default for JobStatus {
  fn default() -> Self {
    JobStatus { open: false, online: true, unseen: false, later: None, anchor: egui::Rect::NOTHING }
  }
}

/// What the panel's card shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Card {
  /// A newer release, downloaded or not: Fusion's "has landed" card.
  Landed {
    version: String,
    page: String,
  },
  Downloading {
    version: String,
    fraction: Option<f32>,
  },
  Checking,
  UpToDate,
  Failed(String),
  Offline,
}

/// The card for the updater's `status`, unless Update Later put the release off past `now`.
pub fn card_for(status: &Status, later: Option<&(String, u64)>, now: u64, online: bool) -> Option<Card> {
  let put_off = |v: &str| later.is_some_and(|(l, until)| l == v && now < *until);
  match status {
    Status::Available(r) | Status::Ready { release: r, .. } => {
      let version = r.version.to_string();
      (!put_off(&version)).then(|| Card::Landed { version, page: r.page.clone() })
    }
    Status::Downloading { release, done, total } => Some(Card::Downloading {
      version: release.version.to_string(),
      fraction: total.filter(|t| *t > 0).map(|t| (*done as f32 / t as f32).clamp(0.0, 1.0)),
    }),
    _ if !online => Some(Card::Offline),
    Status::Checking => Some(Card::Checking),
    Status::UpToDate => Some(Card::UpToDate),
    Status::Failed(e) => Some(Card::Failed(e.clone())),
    Status::Idle => None,
  }
}

/// The card the panel shows now.
pub fn card(app: &App) -> Option<Card> {
  let j = &app.jobs;
  card_for(&app.updates.status(), j.later.as_ref(), fox_engine::store::now(), j.online)
}

/// The number on the icon's badge: 1 while a new version waits, or a check's result hasn't
/// been seen; 0 shows none.
pub fn badge(app: &App) -> usize {
  match card(app) {
    Some(Card::Landed { .. }) => 1,
    Some(_) if app.jobs.unseen => 1,
    _ => 0,
  }
}

/// Help ▸ Check for Updates: looks now (online) and reports on the icon, opening no window.
pub fn check_for_updates(app: &mut App) {
  app.jobs.later = None;
  app.jobs.unseen = true;
  if app.jobs.online {
    app.updates.check_now();
  }
}

/// The updater found a release to announce: the badge shows, as Fusion's.
pub fn announce(app: &mut App) {
  app.jobs.unseen = true;
}

pub fn toggle(app: &mut App) {
  app.jobs.open = !app.jobs.open;
  if app.jobs.open {
    app.jobs.unseen = false;
  }
}

/// Update Later: the card goes for three days.
pub fn update_later(app: &mut App, version: &str) {
  app.jobs.later = Some((version.to_string(), fox_engine::store::now() + LATER));
  app.jobs.open = false;
}

/// Update Now: downloads the release, or installs the downloaded one and restarts (a design with
/// unsaved changes must be saved first). Without an installer for this platform it opens the
/// release's page.
fn update_now(app: &mut App, ctx: &egui::Context, page: &str) {
  let status = app.updates.status();
  match &status {
    Status::Available(r) if r.can_install() => app.updates.download(),
    Status::Available(_) => {
      let _ = fox_desktop::open(page);
    }
    Status::Ready { .. } if app.s.dirty => {
      app.status = Some(("Save your design before FissionOxide restarts to update".into(), true));
    }
    Status::Ready { .. } => match app.updates.install(InstallStyle::Restart) {
      Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
      Err(e) => app.status = Some((e, true)),
    },
    _ => {}
  }
}

/// Draws the icon's badge: a "1" in a red disc at its top right.
pub fn paint_badge(app: &App, p: &egui::Painter, icon: egui::Rect) {
  let n = badge(app);
  if n == 0 {
    return;
  }
  let c = icon.center() + vec2(7.0, -7.0);
  p.circle_filled(c, 6.5, theme::ERROR);
  p.text(c, egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(9.5), Color32::WHITE);
}

enum Act {
  None,
  Now(String),
  Later(String),
  Page(String),
  Retry,
}

/// The panel, while open. A click anywhere else closes it, as a menu would.
pub fn panel(app: &mut App, ctx: &egui::Context) {
  if !app.jobs.open {
    return;
  }
  let anchor = app.jobs.anchor;
  let card = card(app);
  let mut act = Act::None;
  let mut online = app.jobs.online;
  let r = egui::Area::new(egui::Id::new("job-status"))
    .fixed_pos(anchor.center_bottom() + vec2(0.0, 6.0))
    .pivot(egui::Align2::CENTER_TOP)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(3.0)
        .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(40) })
        .inner_margin(12.0)
        .show(ui, |ui| {
          ui.set_width(320.0);
          match &card {
            Some(Card::Landed { version, page }) => {
              ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::hover());
                icons::draw_color(ui.painter(), r, Icon::Gift);
                ui.vertical(|ui| {
                  ui.label(RichText::new(LANDED).strong());
                  ui.add(egui::Label::new(LANDED_BODY).wrap());
                  ui.horizontal(|ui| {
                    if ui.link("Check out what's new").on_hover_text(page).clicked() {
                      act = Act::Page(page.clone());
                    }
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
                    icons::draw(ui.painter(), r, Icon::ExternalLink, theme::ACCENT);
                  });
                  ui.add_space(4.0);
                  ui.horizontal(|ui| {
                    let now = egui::Button::new(RichText::new("Update Now").color(Color32::WHITE))
                      .fill(theme::ACCENT)
                      .corner_radius(3);
                    if ui.add(now).clicked() {
                      act = Act::Now(page.clone());
                    }
                    if ui.add(egui::Button::new("Update Later").corner_radius(3)).clicked() {
                      act = Act::Later(version.clone());
                    }
                  });
                });
              });
            }
            Some(Card::Downloading { version, fraction }) => {
              ui.label(RichText::new(format!("Downloading FissionOxide {version}…")).strong());
              ui.add(egui::ProgressBar::new(fraction.unwrap_or(0.0)).animate(fraction.is_none()));
            }
            Some(Card::Checking) => {
              ui.horizontal(|ui| {
                crate::logo::ripple(ui, 18.0);
                ui.label("Looking for a newer version…");
              });
            }
            Some(Card::UpToDate) => {
              ui.label(RichText::new("FissionOxide is up to date.").color(theme::ICON_GREEN));
            }
            Some(Card::Failed(e)) => {
              ui.add(
                egui::Label::new(RichText::new(format!("Couldn't check for updates: {e}.")).color(theme::ERROR)).wrap(),
              );
              if ui.button("Try Again").clicked() {
                act = Act::Retry;
              }
            }
            Some(Card::Offline) => {
              ui.label(RichText::new("Working offline: FissionOxide doesn't look for updates.").color(theme::MUTED));
            }
            None => {
              ui.label(RichText::new("No jobs are running.").color(theme::MUTED));
            }
          }
          ui.add_space(6.0);
          ui.separator();
          ui.horizontal(|ui| {
            ui.label("Working online");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              let (rect, r) = ui.allocate_exact_size(vec2(30.0, 16.0), egui::Sense::click());
              let fill = if online { theme::ACCENT } else { Color32::from_gray(190) };
              ui.painter().rect_filled(rect, 8.0, fill);
              let knob =
                if online { rect.right_center() - vec2(8.0, 0.0) } else { rect.left_center() + vec2(8.0, 0.0) };
              ui.painter().circle_filled(knob, 6.0, Color32::WHITE);
              if r.clicked() {
                online = !online;
              }
            });
          });
        });
    });
  app.jobs.online = online;
  match act {
    Act::Now(page) => update_now(app, ctx, &page),
    Act::Later(v) => update_later(app, &v),
    Act::Page(page) => {
      let _ = fox_desktop::open(&page);
    }
    Act::Retry => check_for_updates(app),
    Act::None => {}
  }
  // a click outside the panel and the icon closes it
  let clicked_outside = ctx.input(|i| {
    i.pointer.any_click()
      && i.pointer.interact_pos().is_some_and(|p| !r.response.rect.contains(p) && !anchor.contains(p))
  });
  if clicked_outside {
    app.jobs.open = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_desktop::update::Release;
  use fox_desktop::version::Version;
  use fox_engine::Session;

  fn release(v: &str) -> Release {
    Release {
      version: Version::parse(v).unwrap(),
      name: format!("FissionOxide {v}"),
      notes: String::new(),
      page: "https://example.invalid/release".into(),
      installer: None,
      checksum: None,
    }
  }

  #[test]
  fn a_new_release_is_fusions_landed_card_until_put_off() {
    let landed = card_for(&Status::Available(release("9.0.0")), None, 100, true);
    assert_eq!(landed, Some(Card::Landed { version: "9.0.0".into(), page: "https://example.invalid/release".into() }));
    // Update Later puts it off for three days, then it is back
    let later = ("9.0.0".to_string(), 100 + LATER);
    assert_eq!(card_for(&Status::Available(release("9.0.0")), Some(&later), 100, true), None);
    assert!(card_for(&Status::Available(release("9.0.0")), Some(&later), 101 + LATER, true).is_some());
    // a newer release than the one put off shows at once
    assert!(card_for(&Status::Available(release("9.1.0")), Some(&later), 100, true).is_some());
    assert_eq!(card_for(&Status::Idle, None, 0, true), None);
    assert_eq!(card_for(&Status::Failed("offline".into()), None, 0, true), Some(Card::Failed("offline".into())));
    assert_eq!(card_for(&Status::UpToDate, None, 0, false), Some(Card::Offline));
  }

  #[test]
  fn check_for_updates_opens_no_window_and_the_icon_opens_the_panel() {
    let mut app = App::with_session(Session::default());
    assert!(app.jobs.online, "Working online starts on");
    check_for_updates(&mut app);
    assert!(!app.updates.open, "no Software Update window");
    assert!(app.jobs.unseen);
    toggle(&mut app);
    assert!(app.jobs.open && !app.jobs.unseen, "opening the panel reads the news");
    let ctx = egui::Context::default();
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0))),
      ..Default::default()
    };
    app.jobs.anchor = egui::Rect::from_min_size(egui::pos2(900.0, 5.0), vec2(28.0, 28.0));
    ctx.run_ui(input, |ui| panel(&mut app, ui.ctx())).drop_without_applying_deltas();
    let r = ctx.memory(|m| m.area_rect(egui::Id::new("job-status"))).expect("the panel is drawn");
    assert!(r.top() >= 33.0 && r.width() >= 320.0, "under the icon: {r:?}");
    update_later(&mut app, "9.0.0");
    assert!(!app.jobs.open, "Update Later closes the panel");
  }
}
