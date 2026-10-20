//! The notification-area (tray) icon on Windows: shows FissionOxide is running, and offers Show,
//! Check for Updates and Quit. Clicking the icon brings the window back.
//!
//! Must be made on the thread running the window's event loop (Windows delivers the icon's
//! messages there).

use std::sync::mpsc;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// What the user picked on the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
  Show,
  CheckForUpdates,
  OpenDataFolder,
  Quit,
}

pub struct Tray {
  icon: TrayIcon,
  update: MenuItem,
  rx: mpsc::Receiver<Action>,
}

impl Tray {
  /// The icon from RGBA pixels. `wake` is called (from any thread) when there is an action to
  /// [`poll`](Tray::poll), so an idle window can repaint and pick it up.
  pub fn new(rgba: Vec<u8>, width: u32, height: u32, wake: impl Fn() + Send + Sync + 'static) -> Result<Tray, String> {
    let show = MenuItem::new("Show FissionOxide", true, None);
    let update = MenuItem::new("Check for Updates…", true, None);
    let data = MenuItem::new("Open Data Folder", true, None);
    let quit = MenuItem::new("Quit FissionOxide", true, None);
    let menu = Menu::new();
    let sep = PredefinedMenuItem::separator();
    for item in [&show as &dyn tray_icon::menu::IsMenuItem, &sep, &update, &data, &sep, &quit] {
      menu.append(item).map_err(|e| e.to_string())?;
    }
    let icon = tray_icon::Icon::from_rgba(rgba, width, height).map_err(|e| e.to_string())?;
    let icon = TrayIconBuilder::new()
      .with_icon(icon)
      .with_tooltip(format!("FissionOxide {}", crate::VERSION))
      .with_menu(Box::new(menu))
      .with_menu_on_left_click(false)
      .build()
      .map_err(|e| e.to_string())?;

    let (tx, rx) = mpsc::channel();
    let wake = std::sync::Arc::new(wake);
    let ids = [
      (show.id().clone(), Action::Show),
      (update.id().clone(), Action::CheckForUpdates),
      (data.id().clone(), Action::OpenDataFolder),
      (quit.id().clone(), Action::Quit),
    ];
    let (menu_tx, menu_wake) = (tx.clone(), wake.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
      if let Some((_, a)) = ids.iter().find(|(id, _)| *id == e.id) {
        let _ = menu_tx.send(*a);
        menu_wake();
      }
    }));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
      let show = matches!(
        e,
        TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
          | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
      );
      if show {
        let _ = tx.send(Action::Show);
        wake();
      }
    }));
    Ok(Tray { icon, update, rx })
  }

  /// The actions picked since the last call.
  pub fn poll(&self) -> Vec<Action> {
    self.rx.try_iter().collect()
  }

  pub fn set_tooltip(&self, text: &str) {
    let _ = self.icon.set_tooltip(Some(text));
  }

  /// The update entry's label, e.g. "Restart to Update (0.2.0)".
  pub fn set_update_label(&self, text: &str) {
    if self.update.text() != text {
      self.update.set_text(text);
    }
  }
}

impl Drop for Tray {
  fn drop(&mut self) {
    MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
  }
}
