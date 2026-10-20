//! FissionOxide desktop app.
//!
//! `fissionoxide [design.fox]`

// a release build is a windowed app on Windows: no console window behind it
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
  let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
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
  eframe::run_native("FissionOxide", options, Box::new(move |cc| Ok(Box::new(Desktop::new(cc, open)))))
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
}

impl Desktop {
  fn new(cc: &eframe::CreationContext<'_>, open: Option<std::path::PathBuf>) -> Desktop {
    let app = fox_ui::App::new(cc, open);
    #[cfg(windows)]
    {
      let ctx = cc.egui_ctx.clone();
      let tray = eframe::icon_data::from_png_bytes(include_bytes!("../../../brand/logo/png/icon-32.png"))
        .map_err(|e| e.to_string())
        .and_then(|i| fox_desktop::tray::Tray::new(i.rgba, i.width, i.height, move || ctx.request_repaint()));
      let tray = tray.map_err(|e| log::warn!("no notification-area icon: {e}")).ok();
      Desktop { app, splash: Some(Default::default()), tray, tray_text: Default::default() }
    }
    #[cfg(not(windows))]
    Desktop { app, splash: Some(Default::default()) }
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
          self.app.updates.check_now();
        }
        Action::OpenDataFolder => {
          let root = self.app.hub_root();
          let _ = std::fs::create_dir_all(&root);
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
  fn logic(&mut self, _ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
    #[cfg(windows)]
    self.tray(_ctx);
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
