//! Keeps the window's pointer position known after the cursor comes back without moving.
//!
//! When another window covers a still cursor (a notification, a remote-desktop or automation
//! overlay, a window dragged past) and then goes away, Windows sends `WM_MOUSELEAVE` and then a
//! `WM_MOUSEMOVE` at the same spot. winit forgets nothing on the leave but reports a move only
//! when the position changed, so it sends `CursorLeft` and then just `CursorEntered`. egui-winit
//! clears its pointer position on `CursorLeft` and drops every button press while it has none, so
//! the next click on that spot is lost; only moving the mouse brings clicks back.
//!
//! This subclasses the window: after a leave, the first mouse message (a move or a press) is
//! preceded by a move one pixel off and then one at the real spot, so winit reports the position
//! again before the press.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};

type Hwnd = isize;
type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize;

const GWLP_WNDPROC: i32 = -4;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSELEAVE: u32 = 0x02A3;

#[link(name = "user32")]
unsafe extern "system" {
  fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
  fn CallWindowProcW(prev: isize, hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
}

/// The window's own procedure, called for everything.
static PREVIOUS: AtomicIsize = AtomicIsize::new(0);
/// The subclassed window.
static WINDOW: AtomicUsize = AtomicUsize::new(0);
/// The cursor left the window and no move has been reported since.
static LEFT: AtomicBool = AtomicBool::new(false);

/// Installs the fix on the app's main window (its `HWND`). Only the first window gets it.
pub fn keep_position(hwnd: isize) {
  if hwnd == 0 || WINDOW.compare_exchange(0, hwnd as usize, Ordering::SeqCst, Ordering::SeqCst).is_err() {
    return;
  }
  // SAFETY: `hwnd` is a live window of this process, and `procedure` has the window-procedure
  // signature and forwards every message to the procedure it replaces.
  let previous = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, procedure as WndProc as usize as isize) };
  if previous == 0 {
    log::warn!("couldn't watch the window's pointer messages");
    WINDOW.store(0, Ordering::SeqCst);
    return;
  }
  PREVIOUS.store(previous, Ordering::SeqCst);
}

/// Mouse messages that carry the cursor's client position in `lparam`: moves and button presses
/// and releases (not the wheel, whose position is on the screen).
fn has_client_position(msg: u32) -> bool {
  (WM_MOUSEMOVE..=0x0209).contains(&msg) || (0x020B..=0x020D).contains(&msg)
}

/// Whether `msg` must be preceded by a reminder of the position, noting leaves in `left`.
fn needs_reminder(msg: u32, left: &AtomicBool) -> bool {
  if msg == WM_MOUSELEAVE {
    left.store(true, Ordering::SeqCst);
    return false;
  }
  has_client_position(msg) && left.swap(false, Ordering::SeqCst)
}

/// `lparam` for a client position one pixel to the right of the one in `lparam`.
fn nudged(lparam: isize) -> isize {
  let x = (lparam & 0xFFFF) as u16 as i16;
  let y = (lparam >> 16) & 0xFFFF;
  (y << 16) | (x.wrapping_add(1) as u16 as isize)
}

unsafe extern "system" fn procedure(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize {
  let previous = PREVIOUS.load(Ordering::SeqCst);
  if needs_reminder(msg, &LEFT) {
    // SAFETY: forwarding synthetic moves to the window's own procedure, on its own thread.
    unsafe {
      CallWindowProcW(previous, hwnd, WM_MOUSEMOVE, wparam, nudged(lparam));
      if msg != WM_MOUSEMOVE {
        CallWindowProcW(previous, hwnd, WM_MOUSEMOVE, wparam, lparam);
      }
    }
  }
  // SAFETY: as above, with the message as it came.
  unsafe { CallWindowProcW(previous, hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
  use super::*;

  const WM_LBUTTONDOWN: u32 = 0x0201;
  const WM_MOUSEWHEEL: u32 = 0x020A;

  #[test]
  fn the_first_mouse_message_after_a_leave_is_reminded() {
    let left = AtomicBool::new(false);
    assert!(!needs_reminder(WM_MOUSEMOVE, &left));
    assert!(!needs_reminder(WM_MOUSELEAVE, &left));
    assert!(!needs_reminder(WM_MOUSEWHEEL, &left), "the wheel has no client position");
    assert!(needs_reminder(WM_LBUTTONDOWN, &left));
    assert!(!needs_reminder(WM_MOUSEMOVE, &left), "once only");
  }

  #[test]
  fn nudging_moves_one_pixel_right() {
    let at = |x: i16, y: i16| ((y as u16 as isize) << 16) | x as u16 as isize;
    assert_eq!(nudged(at(10, 20)), at(11, 20));
    assert_eq!(nudged(at(-1, 5)), at(0, 5));
    assert_eq!(nudged(at(300, -2)), at(301, -2));
  }
}
