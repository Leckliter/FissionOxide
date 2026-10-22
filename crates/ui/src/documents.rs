//! Document tabs, as Fusion's: every open design has its own tab in the application bar, with
//! its own session (design, undo history, camera); the "+" after the tabs opens "What do you
//! want to design?" and Create New adds a tab, leaving the others open as they were. Closing a
//! tab with unsaved changes asks first, and opening a design while the open one has unsaved
//! changes opens it in a new tab, so no work is dropped without asking.

use egui::{Color32, Rect, RichText, Stroke, Vec2, pos2, vec2};
use fox_engine::model::{DesignType, LengthUnit};
use fox_engine::render::Camera;
use fox_engine::{Session, model::FeatureId};

use crate::file_dialogs;
use crate::icons::{self, Icon};
use crate::panels::Tab;
use crate::{App, Tool, panels, theme};

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
}

/// The open documents. The shown one lives in `App::s` (its slot here is `None`); the others
/// wait in their slots, in tab order.
pub struct Documents {
  slots: Vec<Option<Doc>>,
  active: usize,
  /// "What do you want to design?", while open.
  pub(crate) chooser: Option<Chooser>,
  /// The tab whose close waits on "Save changes?".
  pub(crate) closing: Option<usize>,
}

impl Default for Documents {
  fn default() -> Self {
    Documents { slots: vec![None], active: 0, chooser: None, closing: None }
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

/// A document tab's label: "Untitled" (with a lock) until saved, then "Name v1", starred when
/// dirty.
pub(crate) fn tab_label(s: &Session) -> String {
  if unsaved(s) {
    doc_name(s)
  } else {
    let star = if s.dirty { "*" } else { "" };
    format!("{} v{}{star}", doc_name(s), s.hub.as_ref().map(|h| h.version).unwrap_or(1))
  }
}

/// Whether closing the document would lose work: it has changes not saved anywhere.
pub(crate) fn has_unsaved_work(s: &Session) -> bool {
  s.dirty
}

// ---------------------------------------------------------------- switching

/// Takes the shown document out of the app, leaving a blank session (same hub) in its place.
fn park(app: &mut App) -> Doc {
  app.cancel_dialog();
  app.doc_dialog = None;
  let drawing = app.drawing;
  let tab = app.tab;
  // back to the Design workspace: each document opens in it
  app.electronics = None;
  app.simulation = None;
  app.manufacture = false;
  app.generative = false;
  app.render.active = false;
  app.anim = crate::anim_ui::AnimUi::default();
  app.form.active = false;
  app.drawing = None;
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
  if let Some(d) = app.docs.slots[i].take() {
    unpark(app, d);
  }
}

/// Parks the shown document and adds an empty tab after the others, shown.
fn add_tab(app: &mut App) {
  let doc = park(app);
  let active = app.docs.active;
  app.docs.slots[active] = Some(doc);
  app.docs.slots.push(None);
  app.docs.active = app.docs.slots.len() - 1;
}

/// Create New: a new design of type `ty` in a new tab; the open documents stay as they are.
pub(crate) fn new_document(app: &mut App, ty: DesignType, unit: Option<LengthUnit>) {
  add_tab(app);
  app.new_design_of(ty, unit);
}

/// Before a design is opened in place of the shown one: if the shown one has unsaved changes,
/// it moves to its own tab and the design opens in a new one. Returns the kept tab when it did.
pub(crate) fn keep_unsaved(app: &mut App) -> Option<usize> {
  if !has_unsaved_work(&app.s) {
    return None;
  }
  let kept = app.docs.active;
  add_tab(app);
  Some(kept)
}

/// Undoes [`keep_unsaved`] when the open failed: the new empty tab goes, the kept one (`back`)
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
pub(crate) fn request_close(app: &mut App, i: usize) {
  if i >= app.docs.slots.len() {
    return;
  }
  switch_to(app, i);
  if has_unsaved_work(&app.s) {
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

// ---------------------------------------------------------------- the tab strip

/// The document tabs in `w` (one fills it; more share it), then the "+".
pub(crate) fn tab_strip(app: &mut App, ui: &mut egui::Ui, w: f32) {
  let n = app.docs.slots.len();
  let each = (w / n as f32).floor();
  let mut switch = None;
  let mut close = None;
  for i in 0..n {
    let shown = i == app.docs.active;
    let (label, locked) = match &app.docs.slots[i] {
      Some(d) => (tab_label(&d.s), unsaved(&d.s)),
      None => (tab_label(&app.s), unsaved(&app.s)),
    };
    let (clicked, closed) = document_tab(ui, i, each, &label, locked, shown);
    if clicked {
      switch = Some(i);
    }
    if closed {
      close = Some(i);
    }
  }
  ui.add_space(4.0);
  let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), egui::Sense::click());
  ui.painter().rect_filled(r, 2.0, if resp.hovered() { Color32::from_gray(192) } else { Color32::from_gray(205) });
  icons::draw(ui.painter(), r.shrink(8.0), Icon::Plus, Color32::from_gray(70));
  if resp.on_hover_text(PLUS_TIP).clicked() {
    open_chooser(app);
  }
  if let Some(i) = close {
    request_close(app, i);
  } else if let Some(i) = switch {
    switch_to(app, i);
  }
}

/// The "+" button's tooltip.
pub(crate) const PLUS_TIP: &str = "New Design";

/// One tab: the component icon, the name centred, a lock while unsaved and an X. The shown tab
/// merges into the toolbar below; the others are darker. Returns (clicked, X clicked).
fn document_tab(ui: &mut egui::Ui, i: usize, w: f32, label: &str, locked: bool, shown: bool) -> (bool, bool) {
  let full = ui.max_rect();
  let (slot, resp) = ui.allocate_exact_size(vec2(w, 30.0), egui::Sense::click());
  // the tab runs to the bottom edge of the bar and merges into the toolbar
  let rect = Rect::from_min_max(pos2(slot.min.x, full.bottom() - 33.0), pos2(slot.max.x - 1.0, full.bottom() + 1.0));
  let fill = if shown {
    theme::TOOLBAR
  } else if resp.hovered() {
    Color32::from_gray(214)
  } else {
    Color32::from_gray(200)
  };
  let p = ui.painter();
  p.rect_filled(rect, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, fill);
  let font = egui::FontId::proportional(13.5);
  let galley = p.layout_no_wrap(label.to_string(), font, theme::TEXT);
  let tx = (rect.center().x - galley.size().x / 2.0 + 11.0).max(rect.left() + 30.0);
  icons::draw_color(p, Rect::from_center_size(pos2(tx - 14.0, rect.center().y), Vec2::splat(17.0)), Icon::Component);
  let text_w = galley.size().x;
  p.with_clip_rect(rect.shrink2(vec2(26.0, 0.0))).galley(
    pos2(tx, rect.center().y - galley.size().y / 2.0),
    galley,
    theme::TEXT,
  );
  if locked {
    // an unsaved design's tab carries a lock after its name, as in Fusion
    crate::panels::lock_glyph(p, pos2(tx + text_w + 11.0, rect.center().y), theme::GROUP_LABEL);
  }
  let close = Rect::from_center_size(rect.right_center() - vec2(16.0, 0.0), Vec2::splat(18.0));
  let cr = ui.interact(close, ui.id().with(("doc-close", i)), egui::Sense::click());
  if cr.hovered() {
    ui.painter().rect_filled(close, 2.0, Color32::from_gray(222));
  }
  icons::draw(ui.painter(), close, Icon::Close, theme::ICON_INK);
  let closed = cr.on_hover_text("Close").clicked();
  (resp.clicked() && !closed, closed)
}

// ---------------------------------------------------------------- What do you want to design?

/// The chooser's tiles, in Fusion's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewKind {
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

  /// The right pane's description.
  pub fn description(self) -> &'static str {
    match self {
      NewKind::Part => {
        "Design a single part. Sketch and model bodies with solid, surface, mesh, sheet metal and plastic tools, \
         and keep the part's history in the timeline."
      }
      NewKind::Assembly => {
        "Bring components together. Insert parts and subassemblies, place them with joints and constraints, \
         and study how they move."
      }
      NewKind::Hybrid => {
        "Model parts and assemble them in one design: bodies, components and joints together, with every \
         modeling tool."
      }
      NewKind::Drawing => "Document a design with 2D drawing views, dimensions, annotations and parts lists.",
      NewKind::Electronics => "Design a circuit: a schematic and a printed circuit board, linked to a 3D PCB.",
      NewKind::ElectronicsLibrary => "Create and manage symbols, footprints and 3D packages for electronics parts.",
    }
  }

  /// What FissionOxide can't create yet, with why (the tile is greyed).
  pub fn unavailable(self) -> Option<&'static str> {
    match self {
      NewKind::Drawing => {
        Some("New drawings start from a design in FissionOxide: open the design, then the Drawing workspace")
      }
      NewKind::ElectronicsLibrary => Some("Electronics libraries aren't in FissionOxide yet"),
      _ => None,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartType {
  Standard,
  SheetMetal,
}

/// "What do you want to design?": the tile picked, the part type and unit system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chooser {
  pub kind: NewKind,
  pub part: PartType,
  pub unit: LengthUnit,
}

/// The "+": opens the chooser on Part Design in the default units (Preferences ▸ General).
pub(crate) fn open_chooser(app: &mut App) {
  let unit = LengthUnit::parse(app.preferences.values.units.key()).unwrap_or(LengthUnit::Mm);
  app.docs.chooser = Some(Chooser { kind: NewKind::Part, part: PartType::Standard, unit });
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
    None => {
      new_document(app, DesignType::Part, Some(c.unit));
      if c.kind == NewKind::Electronics {
        panels::switch_workspace(app, "Electronics");
      }
    }
  }
}

/// The open modal windows: the chooser and the close prompt.
pub(crate) fn windows(app: &mut App, ctx: &egui::Context) {
  chooser_window(app, ctx);
  close_prompt(app, ctx);
}

const TILE: Vec2 = vec2(132.0, 108.0);

fn chooser_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut c) = app.docs.chooser else { return };
  let mut create_new = false;
  let mut cancel = false;
  let (_, closed) = file_dialogs::modal(ctx, "new-design", "What do you want to design?", 760.0, |ui| {
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
      // the picked tile's description, Type and Unit System
      ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(c.kind.label()).strong().size(15.0));
        ui.add_space(4.0);
        ui.label(RichText::new(c.kind.description()).color(theme::TEXT));
        if let Some(why) = c.kind.unavailable() {
          ui.add_space(4.0);
          ui.label(RichText::new(why).color(theme::MUTED));
        }
        ui.add_space(12.0);
        if c.kind == NewKind::Part {
          ui.label(RichText::new("Type").color(theme::GROUP_LABEL));
          ui.horizontal(|ui| {
            for (t, icon, tip) in
              [(PartType::Standard, Icon::Body, "Part"), (PartType::SheetMetal, Icon::Flange, "Sheet Metal Part")]
            {
              if icons::icon_button(ui, icon, 28.0, tip, c.part == t, None).clicked() {
                c.part = t;
              }
            }
          });
          ui.add_space(8.0);
        }
        if c.kind.unavailable().is_none() {
          ui.label(RichText::new("Unit System").color(theme::GROUP_LABEL));
          egui::ComboBox::from_id_salt("new-design-units")
            .width(220.0)
            .selected_text(unit_system_label(c.unit))
            .show_ui(ui, |ui| {
              for u in LengthUnit::ALL {
                ui.selectable_value(&mut c.unit, u, unit_system_label(u));
              }
            });
        }
      });
    });
    let b = file_dialogs::footer(ui, Some("Learn More"), "Create New", c.kind.unavailable().is_none());
    if b.left {
      crate::help::open(app, crate::help::DOCS);
    }
    create_new |= b.primary;
    cancel = b.cancel;
  });
  if closed || cancel {
    app.docs.chooser = None;
  } else if create_new {
    create(app, c);
  } else {
    app.docs.chooser = Some(c);
  }
}

/// A chooser tile: a picture and the label, outlined blue when picked, greyed when FissionOxide
/// can't make it.
fn tile(ui: &mut egui::Ui, k: NewKind, picked: bool) -> egui::Response {
  let (rect, r) = ui.allocate_exact_size(TILE, egui::Sense::click());
  let off = k.unavailable().is_some();
  let p = ui.painter();
  let fill = if picked {
    Color32::from_rgb(232, 242, 252)
  } else if r.hovered() {
    theme::HOVER
  } else {
    Color32::WHITE
  };
  let stroke = if picked { Stroke::new(2.0, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
  p.rect(rect, 4.0, fill, stroke, egui::StrokeKind::Inside);
  let pic = Rect::from_center_size(rect.center() - vec2(0.0, 12.0), Vec2::splat(44.0));
  if off {
    icons::draw(p, pic, k.icon(), Color32::from_gray(185));
  } else {
    icons::draw_color(p, pic, k.icon());
  }
  p.text(
    pos2(rect.center().x, rect.bottom() - 18.0),
    egui::Align2::CENTER_CENTER,
    k.label(),
    egui::FontId::proportional(12.5),
    if off { theme::MUTED } else { theme::TEXT },
  );
  match k.unavailable() {
    Some(why) => r.on_hover_text(why),
    None => r,
  }
}

// ---------------------------------------------------------------- closing an unsaved tab

/// "Save changes?" for a tab closing with unsaved changes: Save (keeps the tab and saves),
/// Don't Save (closes it) or Cancel.
fn close_prompt(app: &mut App, ctx: &egui::Context) {
  let Some(i) = app.docs.closing else { return };
  let name = doc_name(&app.s);
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
