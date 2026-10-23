//! Document tabs, as Fusion's: every open design has its own tab in the application bar, with
//! its own session (design, undo history, camera); the "+" after the tabs opens "What do you
//! want to design?" and Create New adds a tab, leaving the others open as they were. Closing a
//! tab with unsaved changes asks first. Opening a design gives it a tab of its own (only a blank,
//! never-edited design is replaced), and a design that is already open shows its tab.

use egui::{Color32, Rect, RichText, Stroke, Vec2, pos2, vec2};
use fox_engine::model::{DesignType, LengthUnit};
use fox_engine::render::Camera;
use fox_engine::{Session, model::FeatureId};

use crate::file_dialogs;
use crate::icons::{self, Icon};
use crate::panels::Tab;
use crate::{App, Tool, theme};

/// A document that isn't the one shown: its session and the view state that goes with it.
pub(crate) struct Doc {
  s: Session,
  cam: Camera,
  drawing: Option<u32>,
  tab: Tab,
  hidden_profiles: Vec<FeatureId>,
  shown_dims: Vec<FeatureId>,
  last_sketch: Option<FeatureId>,
  pre_sketch_cam: Option<Camera>,
  fitted_once: bool,
  /// The browser's Analysis folder: its analyses, sections and eye.
  analyses: crate::analyses::Analyses,
  sections: Vec<crate::dialogs::KeptSection>,
  analysis_hidden: bool,
  /// An electronics document's editor (its start page, schematic or board).
  electronics: Option<crate::ecad_ui::EcadView>,
}

/// The open documents. The shown one lives in `App::s` (its slot here is `None`); the others
/// wait in their slots, in tab order.
pub struct Documents {
  slots: Vec<Option<Doc>>,
  active: usize,
  /// "What do you want to design?", while open.
  pub(crate) chooser: Option<Chooser>,
  /// The tab whose close waits on "Save changes?" (or, never saved, on the Save dialog).
  pub(crate) closing: Option<usize>,
  /// The shown tab scrolls into view in an overflowing tab strip (it just opened or showed).
  reveal: bool,
  /// Where the overflow "v" is, while the tabs don't fit.
  pub(crate) overflow_at: Option<Rect>,
  /// The window's X waits while the unsaved documents ask, one at a time.
  pub(crate) quitting: bool,
  /// While quitting, how many tabs were open when the last one asked: still as many, and that
  /// tab still unsaved, means its prompt was cancelled.
  asked: Option<usize>,
}

impl Default for Documents {
  fn default() -> Self {
    Documents {
      slots: vec![None],
      active: 0,
      chooser: None,
      closing: None,
      reveal: false,
      overflow_at: None,
      quitting: false,
      asked: None,
    }
  }
}

impl Documents {
  /// How many tabs are open.
  pub fn count(&self) -> usize {
    self.slots.len()
  }

  /// The shown tab's index.
  pub fn active(&self) -> usize {
    self.active
  }

  /// Whether the chooser or the close prompt shows: the app's shortcuts wait.
  pub(crate) fn modal_open(&self) -> bool {
    self.chooser.is_some() || self.closing.is_some()
  }
}

// ---------------------------------------------------------------- labels

/// Whether a design has never been saved (no hub version, no file).
pub(crate) fn unsaved(s: &Session) -> bool {
  s.hub.is_none() && s.path.is_none()
}

/// The document's name: its file's stem once saved, else the design's name.
pub(crate) fn doc_name(s: &Session) -> String {
  s.path
    .as_ref()
    .and_then(|p| p.file_stem())
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_else(|| s.design.name.clone())
}

/// A document tab's label: "Untitled" (with a lock) until saved, then "Name v1"; starred when
/// dirty ("Untitled*"), as in Fusion.
pub(crate) fn tab_label(s: &Session) -> String {
  let star = if s.dirty { "*" } else { "" };
  if unsaved(s) {
    format!("{}{star}", doc_name(s))
  } else {
    format!("{} v{}{star}", doc_name(s), s.hub.as_ref().map(|h| h.version).unwrap_or(1))
  }
}

/// The name of a new tab's design, as Fusion numbers them: "Untitled", else the first of
/// "Untitled(1)", "Untitled(2)"... that no open tab has.
pub(crate) fn next_untitled(app: &App) -> String {
  next_untitled_among(app, false)
}

/// [`next_untitled`] among the open designs (`electronics` false) or the open electronics
/// documents: Fusion numbers the two apart, so a new electronics design beside an "Untitled"
/// part design is "Untitled" too.
pub(crate) fn next_untitled_among(app: &App, electronics: bool) -> String {
  let base = fox_engine::model::Design::default().name;
  let names: Vec<String> = app
    .docs
    .slots
    .iter()
    .filter(|d| d.as_ref().map_or(app.electronics.is_some(), |d| d.electronics.is_some()) == electronics)
    .map(|d| doc_name(d.as_ref().map(|d| &d.s).unwrap_or(&app.s)))
    .collect();
  std::iter::once(base.clone())
    .chain((1..).map(|n| format!("{base}({n})")))
    .find(|c| !names.contains(c))
    .unwrap_or(base)
}

/// Whether closing the document would lose work: it has changes not saved anywhere.
pub(crate) fn has_unsaved_work(s: &Session) -> bool {
  s.dirty
}

/// Whether the shown document asks before it closes or is replaced: unsaved changes, or a new
/// Electronics Design never saved (Fusion asks for one even untouched; an untouched new
/// schematic or board closes without asking).
fn asks_first(app: &App) -> bool {
  has_unsaved_work(&app.s) || (asks_untouched(app.electronics.as_ref()) && unsaved(&app.s))
}

/// An electronics document that asks before closing even with no changes: the electronics
/// design on its start page.
fn asks_untouched(e: Option<&crate::ecad_ui::EcadView>) -> bool {
  e.is_some_and(|v| v.page == crate::ecad_chrome::Page::Start)
}

/// Whether closing the shown document asks with the Save dialog (Don't Save added) rather than
/// "Save changes?": a never-saved design, as in Fusion. Electronics documents keep their prompt.
fn saves_on_close(app: &App) -> bool {
  unsaved(&app.s) && app.electronics.is_none()
}

/// [`asks_first`] for a parked document.
fn parked_asks_first(d: &Doc) -> bool {
  has_unsaved_work(&d.s) || (asks_untouched(d.electronics.as_ref()) && unsaved(&d.s))
}

/// The first tab that would ask before closing, if any.
pub(crate) fn first_asking(app: &App) -> Option<usize> {
  (0..app.docs.slots.len()).find(|&i| match &app.docs.slots[i] {
    Some(d) => parked_asks_first(d),
    None => asks_first(app),
  })
}

// ---------------------------------------------------------------- switching

/// Takes the shown document out of the app, leaving a blank session (same hub) in its place.
fn park(app: &mut App) -> Doc {
  app.cancel_dialog();
  app.doc_dialog = None;
  let drawing = app.drawing;
  let tab = app.tab;
  // an electronics document keeps its editor; the others go back to the Design workspace
  let electronics = app.electronics.take();
  app.simulation = None;
  app.manufacture = false;
  app.generative = false;
  app.render.active = false;
  app.anim = crate::anim_ui::AnimUi::default();
  app.form.active = false;
  app.drawing = None;
  // a sketch set aside for a 3D command, or a sketch key's queued command, stays with the old tab
  app.suspended_sketch = None;
  app.sketch_then = None;
  let mut blank = Session::default();
  blank.store_root = app.s.store_root.clone();
  blank.revision = app.s.revision;
  Doc {
    s: std::mem::replace(&mut app.s, blank),
    cam: app.cam,
    drawing,
    tab,
    hidden_profiles: std::mem::take(&mut app.hidden_profiles),
    shown_dims: std::mem::take(&mut app.shown_dims),
    last_sketch: app.last_sketch.take(),
    pre_sketch_cam: app.pre_sketch_cam.take(),
    fitted_once: app.fitted_once,
    analyses: std::mem::take(&mut app.analyses),
    sections: std::mem::take(&mut app.sections),
    analysis_hidden: app.analysis_hidden,
    electronics,
  }
}

/// Shows a parked document.
fn unpark(app: &mut App, d: Doc) {
  let revision = app.s.revision;
  app.s = d.s;
  app.swapped_session(revision);
  app.cam = d.cam;
  app.cam_move = None;
  app.drawing = d.drawing;
  app.tab = d.tab;
  app.hidden_profiles = d.hidden_profiles;
  app.shown_dims = d.shown_dims;
  app.last_sketch = d.last_sketch;
  app.pre_sketch_cam = d.pre_sketch_cam;
  app.fitted_once = d.fitted_once;
  app.analyses = d.analyses;
  app.sections = d.sections;
  app.analysis_hidden = d.analysis_hidden;
  app.electronics = d.electronics;
  app.dialog = None;
  app.tool = Tool::Select;
  app.nav_mode = None;
  app.picked_plane = None;
  app.picked_origin = None;
  app.data.invalidate();
}

/// Shows tab `i`, parking the one shown.
pub(crate) fn switch_to(app: &mut App, i: usize) {
  if i == app.docs.active || i >= app.docs.slots.len() {
    return;
  }
  let doc = park(app);
  let active = app.docs.active;
  app.docs.slots[active] = Some(doc);
  app.docs.active = i;
  app.docs.reveal = true;
  if let Some(d) = app.docs.slots[i].take() {
    unpark(app, d);
  }
}

/// Ctrl+Tab and Ctrl+Shift+Tab: the next (or previous) tab shows, wrapping around, as in Fusion.
pub(crate) fn cycle(app: &mut App, forward: bool) {
  let n = app.docs.slots.len();
  if n < 2 {
    return;
  }
  let i = if forward { (app.docs.active + 1) % n } else { (app.docs.active + n - 1) % n };
  switch_to(app, i);
}

/// Parks the shown document and adds an empty tab after the others, shown.
fn add_tab(app: &mut App) {
  let doc = park(app);
  let active = app.docs.active;
  app.docs.slots[active] = Some(doc);
  app.docs.slots.push(None);
  app.docs.active = app.docs.slots.len() - 1;
  app.docs.reveal = true;
}

/// Create New: a new design of type `ty` in a new tab, numbered "Untitled(n)" when an open tab
/// is already "Untitled"; the open documents stay as they are.
pub(crate) fn new_document(app: &mut App, ty: DesignType, unit: Option<LengthUnit>) {
  let name = next_untitled(app);
  add_tab(app);
  app.new_design_of(ty, unit);
  app.s.design.name = name;
}

/// Whether the shown document is a blank design: never saved, never edited (Fusion's start-up
/// "Untitled", or one just made with "+"). A design opened over it takes its place.
fn blank(app: &App) -> bool {
  unsaved(&app.s) && !app.s.dirty && !app.s.can_undo() && app.electronics.is_none() && app.drawing.is_none()
}

/// Before a design opens: it gets a tab of its own, as in Fusion, leaving the open documents
/// as they are; only a blank design (see [`blank`]) is replaced. Returns the tab shown before
/// when a tab was added.
pub(crate) fn make_room(app: &mut App) -> Option<usize> {
  if blank(app) {
    return None;
  }
  let kept = app.docs.active;
  add_tab(app);
  Some(kept)
}

/// The open tab whose design `is` (a hub item, a file), if one is.
fn tab_where(app: &App, is: impl Fn(&Session) -> bool) -> Option<usize> {
  (0..app.docs.slots.len()).find(|&i| match &app.docs.slots[i] {
    Some(d) => is(&d.s),
    None => is(&app.s),
  })
}

/// Shows the tab that already has hub item `item` (at `version`, or any version when `None`),
/// as Fusion does rather than opening a design twice. Returns whether one did.
pub(crate) fn show_open_item(app: &mut App, item: &str, version: Option<u32>) -> bool {
  let found =
    tab_where(app, |s| s.hub.as_ref().is_some_and(|h| h.item == item && version.is_none_or(|v| v == h.version)));
  found.inspect(|&i| switch_to(app, i)).is_some()
}

/// Shows the tab that already has the design file `path` open. Returns whether one did.
pub(crate) fn show_open_file(app: &mut App, path: &std::path::Path) -> bool {
  let same = |a: &std::path::Path| match (a.canonicalize(), path.canonicalize()) {
    (Ok(a), Ok(b)) => a == b,
    _ => a == path,
  };
  let found = tab_where(app, |s| s.path.as_deref().is_some_and(same));
  found.inspect(|&i| switch_to(app, i)).is_some()
}

/// Undoes [`make_room`] when the open failed: the new empty tab goes, the kept one (`back`)
/// shows again.
pub(crate) fn drop_new_tab(app: &mut App, back: usize) {
  let i = app.docs.active;
  if app.docs.slots.len() < 2 || back >= i {
    return;
  }
  app.docs.slots.remove(i);
  app.docs.active = back;
  if let Some(d) = app.docs.slots[back].take() {
    unpark(app, d);
  }
}

/// The tab's X: closes it, asking first when it has unsaved changes (the tab shows while asked).
/// A never-saved design asks with the Save dialog, Don't Save added, as in Fusion; a saved one
/// with "Save changes?".
pub(crate) fn request_close(app: &mut App, i: usize) {
  if i >= app.docs.slots.len() {
    return;
  }
  switch_to(app, i);
  if asks_first(app) {
    if saves_on_close(app) {
      app.save(false);
    }
    app.docs.closing = Some(i);
  } else {
    close_now(app, i);
  }
}

/// Closes tab `i` (the shown one), dropping its changes. The last tab closing leaves a new
/// empty design, as FissionOxide always shows one.
pub(crate) fn close_now(app: &mut App, i: usize) {
  app.docs.closing = None;
  if i != app.docs.active {
    switch_to(app, i);
  }
  if app.docs.slots.len() == 1 {
    app.new_design();
    app.electronics = None;
    return;
  }
  app.cancel_dialog();
  app.docs.slots.remove(i);
  // the tab to the left shows next, as in Fusion (the first tab's right neighbour)
  let next = i.saturating_sub(1).min(app.docs.slots.len() - 1);
  app.docs.active = next;
  if let Some(d) = app.docs.slots[next].take() {
    unpark(app, d);
  }
}

// ---------------------------------------------------------------- the window's X

/// The window's X, as Fusion's: while a document has unsaved work the close waits, and each
/// such document shows and asks as its tab's X does (the Save dialog with Don't Save for a
/// never-saved design); Cancel keeps the app open with every tab as it was. Once none asks, the
/// window closes.
pub(crate) fn window_close(app: &mut App, ctx: &egui::Context) {
  if ctx.input(|i| i.viewport().close_requested()) && !app.docs.quitting && first_asking(app).is_some() {
    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    app.docs.quitting = true;
    app.docs.asked = None;
    app.docs.chooser = None;
  }
  if !app.docs.quitting || app.docs.closing.is_some() || app.data.save.is_some() {
    return;
  }
  // the last prompt ended with its tab still open and unsaved: it was cancelled
  if app.docs.asked.take().is_some_and(|n| n == app.docs.slots.len()) && asks_first(app) {
    app.docs.quitting = false;
    return;
  }
  match first_asking(app) {
    Some(i) => {
      app.docs.asked = Some(app.docs.slots.len());
      request_close(app, i);
    }
    None => {
      app.docs.quitting = false;
      ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
  }
}

// ---------------------------------------------------------------- the tab strip

/// The narrowest a document tab gets before the strip scrolls: Fusion fits four design tabs
/// (about 187 px each, 142 in the probers' screenshots) in a 1215 px window with no "v", so four
/// fit here at that width too (about 180 px each beside the quick-access buttons).
pub(crate) const TAB_MIN_W: f32 = 180.0;

/// The widest a document tab gets, as Fusion's: one open design doesn't stretch its tab across
/// the bar, the rest of the bar stays grey.
pub(crate) const TAB_MAX_W: f32 = 340.0;

/// The "v" at the end of an overflowing strip, listing every tab.
const OVERFLOW_W: f32 = 22.0;

/// Whether `n` tabs overflow a strip `w` wide (it then scrolls and shows the "v").
pub(crate) fn tabs_overflow(n: usize, w: f32) -> bool {
  n as f32 * TAB_MIN_W > w
}

/// The document tabs in `w` (one fills it; more share it), then the "+". When they don't fit
/// the strip scrolls, keeping the shown tab in view, and a "v" lists them all, as in Fusion.
pub(crate) fn tab_strip(app: &mut App, ui: &mut egui::Ui, w: f32) {
  let n = app.docs.slots.len();
  let labels: Vec<(String, bool, crate::ecad_chrome::DocKind)> = (0..n)
    .map(|i| match &app.docs.slots[i] {
      Some(d) => (tab_label(&d.s), unsaved(&d.s), crate::ecad_chrome::doc_kind(d.electronics.as_ref())),
      None => (tab_label(&app.s), unsaved(&app.s), crate::ecad_chrome::doc_kind(app.electronics.as_ref())),
    })
    .collect();
  let bottom = ui.max_rect().bottom();
  let overflow = tabs_overflow(n, w);
  let reveal = std::mem::take(&mut app.docs.reveal);
  app.docs.overflow_at = None;
  let mut switch = None;
  let mut close = None;
  // on the Home view no document is shown
  let home = app.home.open;
  let mut tabs = |ui: &mut egui::Ui, each: f32| {
    for (i, (label, locked, kind)) in labels.iter().enumerate() {
      let shown = i == app.docs.active && !home;
      let (resp, closed) = document_tab(ui, i, each, bottom, label, *locked, shown, *kind);
      if shown && reveal {
        resp.scroll_to_me(None);
      }
      if resp.clicked() && !closed {
        switch = Some(i);
      }
      if closed {
        close = Some(i);
      }
    }
  };
  if overflow {
    let view_w = (w - OVERFLOW_W - 4.0).max(TAB_MIN_W);
    let h = ui.available_height();
    ui.allocate_ui(vec2(view_w, h), |ui| {
      egui::ScrollArea::horizontal()
        .id_salt("doc-tabs")
        .max_width(view_w)
        .min_scrolled_height(h)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
          // the tabs sit on the bar's bottom edge, as outside the scroll
          let layout = egui::Layout::left_to_right(egui::Align::Max);
          ui.allocate_ui_with_layout(vec2(n as f32 * TAB_MIN_W, h), layout, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            tabs(ui, TAB_MIN_W);
          });
        });
    });
    ui.add_space(4.0);
    let (r, resp) = ui.allocate_exact_size(vec2(OVERFLOW_W, 30.0), egui::Sense::click());
    app.docs.overflow_at = Some(r);
    if resp.hovered() {
      ui.painter().rect_filled(r, 2.0, Color32::from_gray(214));
    }
    icons::chevron(ui.painter(), r.center(), theme::ICON_INK);
    let resp = resp.on_hover_text(OVERFLOW_TIP);
    egui::Popup::menu(&resp).show(|ui| {
      for (i, (label, _, kind)) in labels.iter().enumerate() {
        if document_row(ui, label, *kind, i == app.docs.active).clicked() {
          switch = Some(i);
        }
      }
    });
  } else {
    tabs(ui, (w / n as f32).floor().min(TAB_MAX_W));
  }
  ui.add_space(6.0);
  // Fusion's "+": a plain glyph on the bar, boxed only under the pointer
  let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
  let bg = crate::bar_icons::hover_fill(ui, r, &resp, true);
  let g = Rect::from_center_size(r.center(), Vec2::splat(13.0));
  crate::bar_icons::draw(ui.painter(), g, crate::bar_icons::Glyph::Plus, crate::bar_icons::INK, bg);
  if resp.on_hover_text(PLUS_TIP).clicked() {
    app.home.open = false;
    open_chooser(app);
  }
  // a click on a document's tab leaves the Home view for it
  if switch.is_some() {
    app.home.open = false;
  }
  if let Some(i) = close {
    request_close(app, i);
  } else if let Some(i) = switch {
    switch_to(app, i);
  }
}

/// A row of the "v" list: the document's icon (Fusion's orange design icon; a check mark for the
/// shown one) and its label.
fn document_row(ui: &mut egui::Ui, label: &str, kind: crate::ecad_chrome::DocKind, shown: bool) -> egui::Response {
  let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width().max(220.0), 26.0), egui::Sense::click());
  let p = ui.painter();
  if r.hovered() {
    p.rect_filled(rect, 0.0, theme::HOVER);
  }
  let icon = Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), Vec2::splat(16.0));
  if shown {
    let s = Stroke::new(2.0, theme::TEXT);
    let (a, b, c) = (
      icon.left_center() + vec2(2.0, 0.0),
      icon.center_bottom() + vec2(-1.0, -3.0),
      icon.right_top() + vec2(-2.0, 3.0),
    );
    p.line_segment([a, b], s);
    p.line_segment([b, c], s);
  } else {
    crate::ecad_chrome::doc_icon(p, icon, kind);
  }
  p.text(
    pos2(rect.left() + 32.0, rect.center().y),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(13.0),
    theme::TEXT,
  );
  r
}

/// The overflow "v" button's tooltip.
pub(crate) const OVERFLOW_TIP: &str = "Show all open documents";

/// The "+" button's tooltip.
pub(crate) const PLUS_TIP: &str = "New Design";

/// One tab, as Fusion's: a lock and the name centred (an electronics document shows its icon
/// instead of the lock), and an X at the right. The shown tab is the toolbar's colour and runs
/// down into it (the bar ends at `bottom`), so it reads as part of the window below; the others
/// sit flat on the grey bar, darkening under the pointer. Returns the tab's response and whether
/// its X was clicked.
#[allow(clippy::too_many_arguments)]
fn document_tab(
  ui: &mut egui::Ui,
  i: usize,
  w: f32,
  bottom: f32,
  label: &str,
  locked: bool,
  shown: bool,
  kind: crate::ecad_chrome::DocKind,
) -> (egui::Response, bool) {
  use crate::bar_icons::{self, Glyph};
  let (slot, resp) = ui.allocate_exact_size(vec2(w, 28.0), egui::Sense::click());
  // the tab runs to the bottom edge of the bar and merges into the toolbar
  let rect = Rect::from_min_max(pos2(slot.min.x, bottom - 29.0), pos2(slot.max.x - 1.0, bottom + 1.0));
  let fill = if shown {
    Some(theme::TOOLBAR)
  } else if resp.hovered() {
    Some(Color32::from_gray(206))
  } else {
    None
  };
  let p = ui.painter();
  if let Some(fill) = fill {
    p.rect_filled(rect, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, fill);
  }
  let bg = fill.unwrap_or(theme::APP_BAR);
  let ink = if shown { theme::TEXT } else { Color32::from_gray(70) };
  let font = egui::FontId::proportional(12.5);
  let galley = p.layout_no_wrap(label.to_string(), font, ink);
  let design = kind == crate::ecad_chrome::DocKind::Design;
  // the name centred, the lock (or the document's icon) just before it
  let lead = locked || !design;
  let tx = (rect.center().x - galley.size().x / 2.0 + if lead { 8.0 } else { 0.0 }).max(rect.left() + 30.0);
  let icon_at = pos2(tx - 13.0, rect.center().y);
  if design {
    if locked {
      crate::panels::lock_glyph(p, icon_at - vec2(0.0, 1.0), Color32::from_gray(96));
    }
  } else {
    crate::ecad_chrome::doc_icon(p, Rect::from_center_size(icon_at, Vec2::splat(16.0)), kind);
  }
  p.with_clip_rect(rect.shrink2(vec2(26.0, 0.0))).galley(
    pos2(tx, rect.center().y - galley.size().y / 2.0),
    galley,
    ink,
  );
  let close = Rect::from_center_size(rect.right_center() - vec2(16.0, 0.0), Vec2::splat(18.0));
  let cr = ui.interact(close, ui.id().with(("doc-close", i)), egui::Sense::click());
  let mut cbg = bg;
  if cr.hovered() {
    cbg = if shown { Color32::from_gray(222) } else { bar_icons::HOVER };
    ui.painter().rect_filled(close, 3.0, cbg);
  }
  bar_icons::draw(ui.painter(), close.shrink(5.0), Glyph::Close, Color32::from_gray(90), cbg);
  let closed = cr.on_hover_text("Close").clicked();
  (resp, closed)
}

// ---------------------------------------------------------------- What do you want to design?

/// The chooser's tiles, in Fusion's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewKind {
  #[default]
  Part,
  Assembly,
  Hybrid,
  Drawing,
  Electronics,
  ElectronicsLibrary,
}

impl NewKind {
  pub const ALL: [NewKind; 6] = [
    NewKind::Part,
    NewKind::Assembly,
    NewKind::Hybrid,
    NewKind::Drawing,
    NewKind::Electronics,
    NewKind::ElectronicsLibrary,
  ];

  pub fn label(self) -> &'static str {
    match self {
      NewKind::Part => "Part Design",
      NewKind::Assembly => "Assembly Design",
      NewKind::Hybrid => "Hybrid Design",
      NewKind::Drawing => "Drawing",
      NewKind::Electronics => "Electronics Design",
      NewKind::ElectronicsLibrary => "Electronics Library",
    }
  }

  /// The right pane's description, Fusion's words.
  pub fn description(self) -> &'static str {
    match self {
      NewKind::Part => "Create sketches and bodies to model a single component to manufacture or use in an assembly.",
      NewKind::Assembly => {
        "Create or insert parts and other assemblies as external components, then define relationships between \
         them to create an assembly."
      }
      NewKind::Hybrid => {
        "Create and model internal components, insert external components, and define relationships between them."
      }
      NewKind::Drawing => "Create views and annotations to document the active design for manufacturing.",
      NewKind::Electronics => "Create and manage schematics, 2D PCB, and 3D PCB documents.",
      NewKind::ElectronicsLibrary => "Create and manage components for your electronics designs.",
    }
  }

  /// What FissionOxide can't create yet, with why (the tile is greyed).
  pub fn unavailable(self) -> Option<&'static str> {
    match self {
      NewKind::ElectronicsLibrary => Some("Electronics libraries aren't in FissionOxide yet"),
      _ => None,
    }
  }

  /// The small badge on the tile's picture.
  fn badge(self) -> Color32 {
    match self {
      NewKind::Part | NewKind::Assembly | NewKind::Hybrid => theme::ICON_ORANGE,
      NewKind::Drawing => theme::ACCENT,
      NewKind::Electronics | NewKind::ElectronicsLibrary => theme::ICON_GREEN,
    }
  }

  /// The design type a design tile makes.
  pub fn design_type(self) -> Option<DesignType> {
    match self {
      NewKind::Part => Some(DesignType::Part),
      NewKind::Assembly => Some(DesignType::Assembly),
      NewKind::Hybrid => Some(DesignType::Hybrid),
      _ => None,
    }
  }

  fn icon(self) -> Icon {
    match self {
      NewKind::Part => Icon::Body,
      NewKind::Assembly => Icon::InsertComponent,
      NewKind::Hybrid => Icon::Component,
      NewKind::Drawing => Icon::NewDrawing,
      NewKind::Electronics => Icon::Configure,
      NewKind::ElectronicsLibrary => Icon::Folder,
    }
  }
}

/// A Part Design's Type: a standard part or a sheet metal part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartType {
  #[default]
  Standard,
  SheetMetal,
}

/// "What do you want to design?": the tile picked, the part type and unit system, and a
/// Drawing's Type (a drawing or a template) and Automate.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Chooser {
  pub kind: NewKind,
  pub part: PartType,
  pub unit: LengthUnit,
  pub template: bool,
  pub automate: bool,
}

/// The "+": opens the chooser on Part Design in the default units (Preferences ▸ General).
pub(crate) fn open_chooser(app: &mut App) {
  let unit = LengthUnit::parse(app.preferences.values.units.key()).unwrap_or(LengthUnit::Mm);
  app.docs.chooser = Some(Chooser { unit, automate: true, ..Default::default() });
}

/// "Inch (in), ouncemass", as the chooser's Unit System list shows a unit.
pub(crate) fn unit_system_label(u: LengthUnit) -> String {
  format!("{} ({}), {}", u.label(), u.symbol(), mass_name(u))
}

fn mass_name(u: LengthUnit) -> &'static str {
  match u.mass_symbol() {
    "g" => "gram",
    "kg" => "kilogram",
    "lbmass" => "poundmass",
    other => other,
  }
}

/// Create New: makes what the chooser picked.
pub(crate) fn create(app: &mut App, c: Chooser) {
  app.docs.chooser = None;
  if c.kind.unavailable().is_some() {
    return;
  }
  match c.kind.design_type() {
    Some(ty) => {
      new_document(app, ty, Some(c.unit));
      if ty == DesignType::Part && c.part == PartType::SheetMetal {
        app.tab = Tab::SheetMetal;
      }
    }
    None if c.kind == NewKind::Electronics => crate::ecad_chrome::new_electronics_design(app, Some(c.unit)),
    // a drawing of the shown design, as Fusion's "Design: Untitled": Create Drawing opens on it
    None if c.kind == NewKind::Drawing => app.start(crate::dialogs::Kind::NewDrawing),
    None => new_document(app, DesignType::Part, Some(c.unit)),
  }
}

/// The open modal windows: the chooser and the close prompt.
pub(crate) fn windows(app: &mut App, ctx: &egui::Context) {
  chooser_window(app, ctx);
  close_prompt(app, ctx);
}

/// A tile: a white picture over a grey label band.
pub(crate) const TILE: Vec2 = vec2(104.0, 145.0);
/// The chooser's width, as Fusion's.
pub(crate) const CHOOSER_W: f32 = 640.0;
/// The labels' column in the right pane ("Type", "Unit System").
const LABEL_W: f32 = 78.0;

/// The Type buttons' tooltips: (Type, title, description).
pub(crate) const PART_TYPES: [(PartType, &str, &str); 2] = [
  (PartType::Standard, "Standard", "Creates a standard container for design elements like sketches and bodies."),
  (
    PartType::SheetMetal,
    "Sheet Metal",
    "Creates a component that is subject to sheet metal rules and can be used to create flat patterns.",
  ),
];

/// A row of the right pane: the label at the left, its control after it.
fn pane_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
  ui.horizontal(|ui| {
    ui.allocate_ui_with_layout(vec2(LABEL_W, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
      ui.set_min_width(LABEL_W);
      ui.label(RichText::new(label).color(theme::TEXT));
    });
    add(ui);
  });
}

/// A full-width rule, as under Fusion's title and above its buttons.
fn rule(ui: &mut egui::Ui) {
  ui.add(egui::Separator::default().horizontal().spacing(10.0).grow(file_dialogs::MARGIN));
}

/// A small lightning bolt, Automate's icon.
fn bolt(p: &egui::Painter, c: egui::Pos2) {
  let pts = [vec2(1.5, -7.0), vec2(-4.0, 1.0), vec2(-0.5, 1.0), vec2(-1.5, 7.0), vec2(4.0, -1.0), vec2(0.5, -1.0)]
    .map(|v| c + v);
  p.add(egui::Shape::convex_polygon(pts[..3].to_vec(), theme::ICON_ORANGE, Stroke::NONE));
  p.add(egui::Shape::convex_polygon(pts[3..].to_vec(), theme::ICON_ORANGE, Stroke::NONE));
  p.line_segment([pts[2], pts[5]], Stroke::new(2.0, theme::ICON_ORANGE));
}

fn chooser_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut c) = app.docs.chooser else { return };
  let mut create_new = false;
  let mut cancel = false;
  let mut learn_more = false;
  let design = doc_name(&app.s);
  let (_, closed) = file_dialogs::modal(ctx, "new-design", "What do you want to design?", CHOOSER_W, |ui| {
    rule(ui);
    ui.horizontal_top(|ui| {
      // the tiles, three to a row
      ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
        for row in NewKind::ALL.chunks(3) {
          ui.horizontal(|ui| {
            for &k in row {
              let r = tile(ui, k, c.kind == k);
              if r.clicked() {
                c.kind = k;
              }
              if r.double_clicked() && k.unavailable().is_none() {
                create_new = true;
              }
            }
          });
        }
      });
      ui.add_space(16.0);
      // the picked tile's description, then its settings, each label left of its control
      ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(RichText::new(c.kind.label()).strong().size(15.0));
        ui.label(RichText::new(c.kind.description()).color(theme::TEXT));
        ui.add_space(6.0);
        match c.kind {
          NewKind::Part => pane_row(ui, "Type", |ui| {
            for (t, title, tip) in PART_TYPES {
              let icon = if t == PartType::Standard { Icon::Body } else { Icon::Flange };
              let r = icons::icon_button(ui, icon, 28.0, "", c.part == t, None).on_hover_ui(|ui| {
                ui.label(RichText::new(title).strong());
                ui.label(tip);
              });
              if r.clicked() {
                c.part = t;
              }
            }
          }),
          NewKind::Drawing => {
            pane_row(ui, "Type", |ui| {
              ui.radio_value(&mut c.template, false, "Drawing");
              ui.add_enabled(false, egui::RadioButton::new(c.template, "Template"))
                .on_disabled_hover_text("Drawing templates aren't in FissionOxide yet");
            });
            pane_row(ui, "Automate", |ui| {
              let mut on = true;
              ui.add_enabled(false, egui::Checkbox::without_text(&mut on))
                .on_disabled_hover_text("FissionOxide's Create Drawing places the base and projected views");
              let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), egui::Sense::hover());
              bolt(ui.painter(), r.center());
            });
            pane_row(ui, "Design", |ui| {
              ui.label(RichText::new(&design).color(theme::TEXT));
            });
          }
          _ => {}
        }
        if c.kind.unavailable().is_none() && c.kind != NewKind::Drawing {
          pane_row(ui, "Unit System", |ui| {
            egui::ComboBox::from_id_salt("new-design-units")
              .width(ui.available_width().min(200.0))
              .selected_text(unit_system_label(c.unit))
              .show_ui(ui, |ui| {
                for u in LengthUnit::ALL {
                  ui.selectable_value(&mut c.unit, u, unit_system_label(u));
                }
              });
          });
        }
      });
    });
    ui.add_space(10.0);
    rule(ui);
    // "Learn More" is a link at the left, Cancel and Create New at the right
    ui.allocate_ui_with_layout(
      vec2(ui.available_width(), file_dialogs::BUTTON.y),
      egui::Layout::left_to_right(egui::Align::Center),
      |ui| {
        let link = ui
          .add(egui::Label::new(RichText::new("Learn More").color(theme::ACCENT)).sense(egui::Sense::click()))
          .on_hover_cursor(egui::CursorIcon::PointingHand);
        learn_more = link.clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          create_new |=
            ui.add_enabled(c.kind.unavailable().is_none(), file_dialogs::blue_button("Create New")).clicked();
          cancel = ui.add(file_dialogs::white_button("Cancel")).clicked();
        });
      },
    );
  });
  if learn_more {
    crate::help::open(app, crate::help::DOCS);
  }
  if closed || cancel {
    app.docs.chooser = None;
  } else if create_new {
    create(app, c);
  } else {
    app.docs.chooser = Some(c);
  }
}

/// A chooser tile: the picture on white with a small coloured badge, the label in a grey band
/// below, outlined blue when picked, greyed when FissionOxide can't make it.
fn tile(ui: &mut egui::Ui, k: NewKind, picked: bool) -> egui::Response {
  let (rect, r) = ui.allocate_exact_size(TILE, egui::Sense::click());
  let off = k.unavailable().is_some();
  let p = ui.painter();
  let band_h = 41.0;
  let pic_area = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - band_h));
  let band = Rect::from_min_max(pos2(rect.left(), rect.bottom() - band_h), rect.max);
  p.rect_filled(pic_area, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, Color32::WHITE);
  let band_fill = if r.hovered() && !picked { Color32::from_gray(226) } else { Color32::from_gray(238) };
  p.rect_filled(band, egui::CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, band_fill);
  let pic = Rect::from_center_size(pic_area.center(), Vec2::splat(52.0));
  if off {
    icons::draw(p, pic, k.icon(), Color32::from_gray(185));
  } else {
    icons::draw_color(p, pic, k.icon());
  }
  let badge = Rect::from_min_size(pic_area.left_bottom() + vec2(6.0, -22.0), Vec2::splat(16.0));
  let badge_fill = if off { Color32::from_gray(200) } else { k.badge() };
  p.rect_filled(badge, 2.0, badge_fill);
  icons::draw(p, badge.shrink(2.0), k.icon(), Color32::WHITE);
  // the label, centred and wrapped in the band
  let mut job = egui::text::LayoutJob::simple(
    k.label().to_string(),
    egui::FontId::proportional(12.5),
    if off { theme::MUTED } else { theme::TEXT },
    band.width() - 8.0,
  );
  job.halign = egui::Align::Center;
  let galley = p.layout_job(job);
  p.galley(pos2(band.center().x, band.center().y - galley.size().y / 2.0), galley, theme::TEXT);
  let stroke = if picked { Stroke::new(2.0, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
  p.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
  match k.unavailable() {
    Some(why) => r.on_hover_text(why),
    None => r,
  }
}

// ---------------------------------------------------------------- closing an unsaved tab

/// "Save changes?" for a saved design's tab closing with unsaved changes: Save (keeps the tab
/// and saves), Don't Save (closes it) or Cancel.
fn close_prompt(app: &mut App, ctx: &egui::Context) {
  let Some(i) = app.docs.closing else { return };
  // a never-saved design asks with the Save dialog ([`crate::data_panel::save_window`])
  if saves_on_close(app) {
    if app.data.save.is_none() {
      app.docs.closing = None;
    }
    return;
  }
  let name = crate::ecad_chrome::close_name(app.electronics.as_ref()).map_or_else(|| doc_name(&app.s), str::to_string);
  let mut act = None;
  let (_, closed) = file_dialogs::modal(ctx, "close-design", "Close", 420.0, |ui| {
    ui.add_space(6.0);
    ui.label(format!("Save changes to {name}?"));
    ui.label(RichText::new("Your changes will be lost if you don't save them.").color(theme::MUTED));
    let b = file_dialogs::footer(ui, Some("Don't Save"), "Save", true);
    if b.left {
      act = Some(false);
    } else if b.primary {
      act = Some(true);
    } else if b.cancel {
      app.docs.closing = None;
    }
  });
  if closed {
    app.docs.closing = None;
  }
  match act {
    Some(true) => {
      app.docs.closing = None;
      app.save(false);
    }
    Some(false) => close_now(app, i),
    None => {}
  }
}
