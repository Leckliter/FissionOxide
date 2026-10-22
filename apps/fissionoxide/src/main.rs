//! FissionOxide desktop app.
//!
//! `fissionoxide [--new-instance] [design.fox]`
//!
//! Launched while it's already running, it hands the file to the running app (which comes to
//! the front) and exits, as Fusion does. `--new-instance` or `$FOX_MULTI_INSTANCE=1` starts
//! another app instead; a separate `$FOX_CONFIG_DIR` is a separate instance too.

// a release build is a windowed app on Windows: no console window behind it
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Log lines go to stderr and to the diagnostic log file (Help > Diagnostic Log).
struct Tee(std::fs::File);

impl std::io::Write for Tee {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    let _ = std::io::stderr().write_all(buf);
    self.0.write_all(buf)?;
    Ok(buf.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.0.flush()
  }
}

/// The diagnostic log for this run, the previous one kept beside it.
fn log_file() -> Option<std::fs::File> {
  let dir = fox_desktop::log_dir();
  std::fs::create_dir_all(&dir).ok()?;
  let path = dir.join("fissionoxide.log");
  let _ = std::fs::rename(&path, dir.join("fissionoxide.prev.log"));
  std::fs::File::create(path).ok()
}

fn main() -> eframe::Result<()> {
  let mut logger =
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,fissionoxide=info"));
  if let Some(f) = log_file() {
    logger.target(env_logger::Target::Pipe(Box::new(Tee(f))));
  }
  logger.init();
  log::info!("FissionOxide {} starting", fox_desktop::VERSION);
  let (flags, files): (Vec<_>, Vec<_>) = std::env::args_os().skip(1).partition(|a| a == "--new-instance");
  let open = files.into_iter().next().map(std::path::PathBuf::from);
  let instance = if flags.is_empty() && !fox_desktop::instance::disabled_by_env() {
    match fox_desktop::instance::claim(open.as_deref()) {
      Some(i) => Some(i),
      // handed to the running app
      None => return Ok(()),
    }
  } else {
    None
  };
  let options = eframe::NativeOptions {
    viewport: eframe::egui::ViewportBuilder::default()
      .with_inner_size([1440.0, 900.0])
      .with_min_inner_size([900.0, 600.0])
      .with_title("FissionOxide")
      .with_app_id("fissionoxide")
      .with_icon(
        eframe::icon_data::from_png_bytes(include_bytes!("../../../brand/logo/png/icon-256.png")).unwrap_or_default(),
      ),
    ..Default::default()
  };
  eframe::run_native("FissionOxide", options, Box::new(move |cc| Ok(Box::new(Desktop::new(cc, open, instance)))))
}

/// The app in its desktop shell: the start-up splash, and on Windows an icon in the notification
/// area while it runs.
struct Desktop {
  app: fox_ui::App,
  /// Until it has faded off the app.
  splash: Option<fox_ui::Splash>,
  #[cfg(windows)]
  tray: Option<fox_desktop::tray::Tray>,
  /// The tray's tooltip and update entry as last set.
  #[cfg(windows)]
  tray_text: (String, String),
  /// Takes later launches' files, unless this is a separate instance.
  instance: Option<fox_desktop::instance::Instance>,
  /// A file a later launch handed over, opened once the window is in front.
  handed: Option<std::path::PathBuf>,
}

impl Desktop {
  fn new(
    cc: &eframe::CreationContext<'_>,
    open: Option<std::path::PathBuf>,
    instance: Option<fox_desktop::instance::Instance>,
  ) -> Desktop {
    let app = fox_ui::App::new(cc, open);
    if let Some(i) = &instance {
      let ctx = cc.egui_ctx.clone();
      i.set_wake(move || ctx.request_repaint());
    }
    #[cfg(windows)]
    {
      // a click where the cursor came back without moving would otherwise be dropped
      use raw_window_handle::{HasWindowHandle, RawWindowHandle};
      if let Ok(h) = cc.window_handle()
        && let RawWindowHandle::Win32(w) = h.as_raw()
      {
        fox_desktop::pointer::keep_position(w.hwnd.get());
      }
      let ctx = cc.egui_ctx.clone();
      let tray = eframe::icon_data::from_png_bytes(include_bytes!("../../../brand/logo/png/icon-32.png"))
        .map_err(|e| e.to_string())
        .and_then(|i| fox_desktop::tray::Tray::new(i.rgba, i.width, i.height, move || ctx.request_repaint()));
      let tray = tray.map_err(|e| log::warn!("no notification-area icon: {e}")).ok();
      Desktop { app, splash: Some(Default::default()), tray, tray_text: Default::default(), instance, handed: None }
    }
    #[cfg(not(windows))]
    Desktop { app, splash: Some(Default::default()), instance, handed: None }
  }

  /// Later launches: brings the window to the front, then (on the next frame, so a prompt shows
  /// over it) opens the file handed over.
  fn handed_over(&mut self, ctx: &eframe::egui::Context) {
    use eframe::egui::ViewportCommand;
    if let Some(p) = self.handed.take() {
      self.open_handed(p);
    }
    let Some(instance) = &self.instance else { return };
    for r in instance.poll() {
      ctx.send_viewport_cmd(ViewportCommand::Visible(true));
      ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
      ctx.send_viewport_cmd(ViewportCommand::Focus);
      if r.open.is_some() {
        self.handed = r.open;
        ctx.request_repaint();
      }
    }
  }

  /// Opens a handed-over file in place of the design, asking first if that has unsaved changes.
  fn open_handed(&mut self, p: std::path::PathBuf) {
    if self.app.s.dirty {
      let name = p.file_name().unwrap_or(p.as_os_str()).to_string_lossy().to_string();
      let discard = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("FissionOxide")
        .set_description(format!("{} has unsaved changes. Discard them and open {name}?", self.app.s.design.name))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show();
      if discard != rfd::MessageDialogResult::Ok {
        return;
      }
    }
    self.app.open_file(p);
  }

  #[cfg(windows)]
  fn tray(&mut self, ctx: &eframe::egui::Context) {
    use eframe::egui::ViewportCommand;
    use fox_desktop::tray::Action;
    let Some(tray) = &self.tray else { return };
    for a in tray.poll() {
      match a {
        Action::Show => {
          ctx.send_viewport_cmd(ViewportCommand::Visible(true));
          ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
          ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        Action::CheckForUpdates => {
          ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
          ctx.send_viewport_cmd(ViewportCommand::Focus);
          // as Help ▸ Check for Updates, it reports in Job Status, shown open
          self.app.updates.check_now();
          self.app.jobs.open = true;
        }
        Action::OpenDataFolder => {
          let root = self.app.hub_root();
          // a hub server's page opens in the browser
          if !fox_ui::is_remote_hub(&root) {
            let _ = std::fs::create_dir_all(&root);
          }
          if let Err(e) = fox_desktop::open(&root.to_string_lossy()) {
            log::warn!("couldn't open {}: {e}", root.display());
          }
        }
        Action::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
      }
    }
    // Windows limits a tooltip to 127 characters
    let mut tip = self.app.title();
    if let Some(l) = self.app.updates.tooltip_line() {
      tip = format!("{tip}\nUpdate: {l}");
    }
    let tip: String = tip.chars().take(127).collect();
    let label = self.app.updates.menu_label();
    if self.tray_text != (tip.clone(), label.clone()) {
      tray.set_tooltip(&tip);
      tray.set_update_label(&label);
      self.tray_text = (tip, label);
    }
  }
}

impl eframe::App for Desktop {
  fn logic(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
    self.handed_over(ctx);
    #[cfg(windows)]
    self.tray(ctx);
  }

  fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
    use fox_ui::SplashPhase;
    let Some(splash) = &mut self.splash else {
      self.app.ui(ui, frame);
      return;
    };
    let ctx = ui.ctx().clone();
    let note = self.app.loading();
    let phase = splash.phase(&ctx, note.is_some());
    // the app draws beneath the finished logo, so its first frames are ready when it fades
    if phase != SplashPhase::Playing {
      self.app.ui(ui, frame);
    }
    if phase == SplashPhase::Done {
      self.splash = None;
    } else {
      splash.show(&ctx, phase, note.as_deref());
    }
  }

  fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
    self.app.on_exit(gl);
  }
}
