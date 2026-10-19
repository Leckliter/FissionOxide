//! FissionOxide desktop app.
//!
//! `fissionoxide [design.fox]`

fn main() -> eframe::Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
  let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
  let options = eframe::NativeOptions {
    viewport: eframe::egui::ViewportBuilder::default()
      .with_inner_size([1440.0, 900.0])
      .with_min_inner_size([900.0, 600.0])
      .with_title("FissionOxide")
      .with_icon(
        eframe::icon_data::from_png_bytes(include_bytes!("../../../brand/logo/png/icon-256.png")).unwrap_or_default(),
      ),
    ..Default::default()
  };
  eframe::run_native("FissionOxide", options, Box::new(move |cc| Ok(Box::new(fox_ui::App::new(cc, open)))))
}
