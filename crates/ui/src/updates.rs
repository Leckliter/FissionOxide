//! Software updates in the UI: the Software Update window (what's new, download progress,
//! install; the bell's update notice opens it), and the setting for how updates happen. Help →
//! Check for Updates reports in Job Status (`crate::job_status`). The checking,
//! downloading and installing is `fox_desktop::update::Updater`'s, on its own thread.

use egui::{Color32, RichText, vec2};
use fox_desktop::settings::UpdateMode;
use fox_desktop::update::{InstallStyle, Status, Updater};

use crate::{App, logo, theme};

#[derive(Default)]
pub struct Updates {
  /// None in tests and headless use: nothing is checked.
  updater: Option<Updater>,
  /// The Software Update window is open.
  pub open: bool,
  /// The release the window last opened by itself for, so it does that once per release.
  announced: Option<String>,
  /// An installer was started from the window; the app is closing for it.
  installing: bool,
}

impl Updates {
  /// Starts the updater: removes stale downloads and, if the user wants it, checks in the
  /// background (at most about once a day). Development builds only check when asked.
  pub fn start(ctx: &egui::Context) -> Updates {
    let c = ctx.clone();
    let updater = Updater::new(move || c.request_repaint());
    if !cfg!(debug_assertions) {
      updater.start();
    }
    Updates { updater: Some(updater), ..Updates::default() }
  }

  pub fn status(&self) -> Status {
    self.updater.as_ref().map_or(Status::Idle, Updater::status)
  }

  /// Help → Check for Updates: looks now. As in Fusion it opens no window; the result shows in
  /// Job Status (`crate::job_status`).
  pub fn check_now(&mut self) {
    if let Some(u) = &self.updater {
      u.check(true);
    }
  }

  /// Job Status's Update Now on a release not downloaded yet.
  pub fn download(&self) {
    if let Some(u) = &self.updater {
      u.download();
    }
  }

  /// Job Status's Update Now on a downloaded release: starts its installer (the app then closes).
  pub fn install(&mut self, style: InstallStyle) -> Result<(), String> {
    let Some(u) = &self.updater else { return Err("FissionOxide isn't set up to update itself".into()) };
    u.install(style)?;
    self.installing = true;
    Ok(())
  }

  /// What the tray icon's update entry says.
  pub fn menu_label(&self) -> String {
    match self.status() {
      Status::Ready { release, .. } => format!("Restart to Update ({})", release.version),
      Status::Available(r) => format!("Update to {}…", r.version),
      Status::Downloading { .. } => "Downloading Update…".to_string(),
      _ => "Check for Updates…".to_string(),
    }
  }

  /// One line for the tray icon's tooltip, when there is news.
  pub fn tooltip_line(&self) -> Option<String> {
    match self.status() {
      Status::Ready { release, .. } => Some(format!("{} is ready to install", release.version)),
      Status::Available(r) => Some(format!("{} is available", r.version)),
      Status::Downloading { release, .. } => Some(format!("Downloading {}", release.version)),
      _ => None,
    }
  }

  /// As the app closes: installs a downloaded update, unless one was already started.
  pub fn on_exit(&self) {
    if !self.installing
      && let Some(u) = &self.updater
      && u.install_on_exit()
    {
      log::info!("installing the downloaded update as FissionOxide closes");
    }
  }
}

fn megabytes(b: u64) -> String {
  format!("{:.1} MB", b as f64 / 1_000_000.0)
}

/// The Software Update window. It also opens by itself once per release: in Notify mode when one
/// is available, in Auto mode when it has downloaded (or, with no installer for this platform, when
/// it is available).
pub fn window(app: &mut App, ctx: &egui::Context) {
  let Some(updater) = app.updates.updater.clone() else { return };
  let status = updater.status();
  let mode = updater.settings().update_mode;
  let announce = match &status {
    // in Auto mode a release with nothing to install here (no Windows installer) is announced too
    Status::Available(r) => mode == UpdateMode::Notify || (mode == UpdateMode::Auto && !r.can_install()),
    Status::Ready { .. } => mode != UpdateMode::Off,
    _ => false,
  };
  if announce
    && let Some(r) = status.release()
    && app.updates.announced.as_deref() != Some(&r.version.to_string())
  {
    app.updates.announced = Some(r.version.to_string());
    // Fusion announces a new version on Job Status's icon, not in a window
    crate::job_status::announce(app);
  }
  if !app.updates.open {
    return;
  }
  let mut open = true;
  let mut close = false;
  let dirty = app.s.dirty;
  egui::Window::new("Software Update")
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .default_width(440.0)
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.set_width(440.0);
      ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(34.0, 38.0), egui::Sense::hover());
        logo::paint(ui.painter(), r);
        ui.vertical(|ui| {
          ui.label(RichText::new("FissionOxide").strong().size(16.0));
          ui.label(RichText::new(format!("Version {}", fox_desktop::VERSION)).color(theme::MUTED));
        });
      });
      ui.add_space(6.0);
      match &status {
        Status::Idle => {
          ui.label("FissionOxide hasn't looked for updates yet.");
          if ui.button("Check Now").clicked() {
            updater.check(true);
          }
        }
        Status::Checking => {
          ui.horizontal(|ui| {
            crate::logo::ripple(ui, 18.0);
            ui.label("Looking for a newer version…");
          });
        }
        Status::UpToDate => {
          ui.label(RichText::new("✔ FissionOxide is up to date.").color(theme::ICON_GREEN));
          if ui.button("Check Again").clicked() {
            updater.check(true);
          }
        }
        Status::Failed(e) => {
          ui.label(RichText::new(format!("Couldn't update: {e}.")).color(theme::ERROR));
          ui.horizontal(|ui| {
            if ui.button("Try Again").clicked() {
              updater.check(true);
            }
            if ui.button("Open Releases Page").clicked() {
              let _ = fox_desktop::open(fox_desktop::update::RELEASES_PAGE);
            }
          });
        }
        Status::Available(r) => {
          ui.label(RichText::new(format!("{} is available.", r.name)).strong());
          notes(ui, &r.notes);
          ui.horizontal(|ui| {
            if r.can_install() {
              let b =
                egui::Button::new(RichText::new("Download and Install").color(Color32::WHITE)).fill(theme::ACCENT);
              if ui.add(b).clicked() {
                updater.download();
              }
            } else if ui.button("Open Download Page").clicked() {
              let _ = fox_desktop::open(&r.page);
            }
            if ui.button("Skip This Version").clicked() {
              updater.skip(r);
              close = true;
            }
            if ui.button("Later").clicked() {
              close = true;
            }
          });
        }
        Status::Downloading { release, done, total } => {
          ui.label(format!("Downloading {}…", release.name));
          let total = total.filter(|t| *t > 0);
          let frac = total.map_or(0.0, |t| *done as f32 / t as f32);
          let text = match total {
            Some(t) => format!("{} of {}", megabytes(*done), megabytes(t)),
            None => megabytes(*done),
          };
          ui.add(egui::ProgressBar::new(frac.clamp(0.0, 1.0)).text(text).animate(total.is_none()));
          ui.label(RichText::new("You can keep working; this window can be closed.").color(theme::MUTED));
        }
        Status::Ready { release, .. } => {
          ui.label(RichText::new(format!("{} is ready to install.", release.name)).strong());
          notes(ui, &release.notes);
          ui.label(
            RichText::new("It installs by itself when you close FissionOxide, or now with Restart and Update.")
              .color(theme::MUTED),
          );
          ui.horizontal(|ui| {
            let restart = ui
              .add_enabled(
                !dirty,
                egui::Button::new(RichText::new("Restart and Update").color(Color32::WHITE)).fill(theme::ACCENT),
              )
              .on_disabled_hover_text("Save your design first");
            if restart.clicked() {
              match updater.install(InstallStyle::Restart) {
                Ok(()) => {
                  app.updates.installing = true;
                  ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => app.status = Some((e, true)),
              }
            }
            if ui.button("Later").clicked() {
              close = true;
            }
          });
          if dirty {
            ui.label(RichText::new("Save your design before restarting.").color(theme::WARNING));
          }
        }
      }
      ui.add_space(4.0);
      ui.separator();
      ui.horizontal(|ui| {
        ui.label("Updates:");
        let mut m = mode;
        egui::ComboBox::from_id_salt("update-mode").width(300.0).selected_text(m.label()).show_ui(ui, |ui| {
          for o in UpdateMode::ALL {
            ui.selectable_value(&mut m, o, o.label());
          }
        });
        if m != mode {
          updater.set_mode(m);
        }
      });
      if ui.link("All releases and release notes").clicked() {
        let _ = fox_desktop::open(fox_desktop::update::RELEASES_PAGE);
      }
    });
  if !open || close {
    app.updates.open = false;
  }
}

/// A release's notes. Of their Markdown, headings show bold and list items with bullets; the
/// rest as written.
fn notes(ui: &mut egui::Ui, notes: &str) {
  if notes.trim().is_empty() {
    return;
  }
  ui.add_space(2.0);
  egui::Frame::new().fill(theme::DIALOG_BODY).stroke(egui::Stroke::new(1.0, theme::BORDER)).inner_margin(8.0).show(
    ui,
    |ui| {
      egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        for line in notes.trim().lines() {
          let t = line.trim();
          if let Some(h) = t.strip_prefix('#') {
            ui.add_space(3.0);
            ui.label(RichText::new(h.trim_start_matches('#').trim()).strong());
          } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            ui.label(format!("  •  {item}"));
          } else if !t.is_empty() {
            ui.label(t);
          }
        }
      });
    },
  );
  ui.add_space(4.0);
}
