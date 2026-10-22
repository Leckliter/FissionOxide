//! The electronics documents, as Fusion's. DESIGN ▸ ELECTRONICS ▸ New Electronics Design (or the
//! "+" chooser's Electronics Design) adds a document tab with the electronics design's start
//! page: COMMON | UTILITIES ribbon tabs, a browser with the "(Unsaved)" root and a Description,
//! a "New Electronics Design" page in the canvas and TEXT COMMANDS open at the bottom. New
//! Schematic (and New PCB) opens another tab with the editor: DESIGN, DOCUMENT, VALIDATE,
//! LIBRARY and UTILITIES ribbon tabs of icon panels, the net class, grid readout and command line
//! under the ribbon, the browser over a white sheet, PLACE COMPONENTS at the right, the canvas
//! toolbar and prompt, and SHEETS, TEXT COMMANDS and COMMENTS along the bottom. The sheet itself
//! is drawn and edited in `ecad_ui`.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2, pos2, vec2};
use fox_engine::electronics::board::Side;
use fox_engine::electronics::geom::P;
use fox_engine::model::{DesignType, LengthUnit};
use serde_json::{Value, json};

use crate::ecad_ui::{self, Cam, EcadView, Sheet, Tool};
use crate::icons::{self, Icon};
use crate::{App, documents, panels, theme};

/// What an electronics tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
  /// The electronics design's start page.
  #[default]
  Start,
  /// The schematic or board editor.
  Editor,
}

/// TEXT COMMANDS' language: Fusion's Txt / Py / TS radio buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextMode {
  #[default]
  Txt,
  Py,
  Ts,
}

/// The kind of document a tab holds, for its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
  Design,
  Electronics,
  Schematic,
  Board,
}

/// The tab icon's kind for a document with this electronics state.
pub(crate) fn doc_kind(e: Option<&EcadView>) -> DocKind {
  match e {
    None => DocKind::Design,
    Some(v) if v.page == Page::Start => DocKind::Electronics,
    Some(v) if v.sheet == Sheet::Board => DocKind::Board,
    Some(_) => DocKind::Schematic,
  }
}

/// The name "Save changes to ...?" gives a document: Fusion calls a new electronics design
/// "Electronics Design".
pub(crate) fn close_name(e: Option<&EcadView>) -> Option<&'static str> {
  e.filter(|v| v.page == Page::Start).map(|_| "Electronics Design")
}

/// The electronics design's ribbon tabs.
pub const START_TABS: [&str; 2] = ["COMMON", "UTILITIES"];
/// The schematic and board editor's ribbon tabs.
pub const EDITOR_TABS: [&str; 5] = ["DESIGN", "DOCUMENT", "VALIDATE", "LIBRARY", "UTILITIES"];

/// The start page's text, as Fusion's.
pub const START_HEADING: &str = "New Electronics Design";
pub const START_LEAD: &str = "Start with a schematic";
/// The command line's hint.
pub const CMDLINE_HINT: &str = "Click or press / to activate command line mode";
/// The net class the command row shows (Fusion's default class, with its green swatch).
pub const NET_CLASS: &str = "sc2 Nets";

// ---------------------------------------------------------------- new documents

/// New Electronics Design: a new document tab with the start page; the open documents stay.
pub(crate) fn new_electronics_design(app: &mut App, unit: Option<LengthUnit>) {
  documents::new_document(app, DesignType::Part, unit);
  app.electronics = Some(EcadView { page: Page::Start, text_open: true, ..EcadView::default() });
}

/// New Schematic / New PCB: another tab with the editor on that sheet.
pub(crate) fn new_editor(app: &mut App, sheet: Sheet) {
  documents::new_document(app, DesignType::Part, None);
  app.electronics = Some(EcadView { page: Page::Editor, sheet, ..EcadView::default() });
}

// ---------------------------------------------------------------- icons

/// A ribbon icon: one of the app's, or an electronics one drawn here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
  Std(Icon),
  Schematic,
  Pcb,
  RefSchematic,
  RefPcb,
  FromDesign,
  Pcb3d,
  Part,
  Net,
  Bus,
  NetLabel,
  NoConnect,
  Check,
  Simulate,
  Layers,
  Route,
  Via,
  Autoroute,
  Rotate,
  Shortcut,
  Text,
  Info,
  Stop,
  ZoomIn,
  ZoomOut,
}

const BOARD_GREEN: Color32 = Color32::from_rgb(46, 140, 70);
const COPPER: Color32 = Color32::from_rgb(200, 52, 52);
const GREYED: Color32 = Color32::from_gray(185);

/// A green circuit board with a chip, in `r`.
fn board_picture(p: &egui::Painter, r: Rect, mono: Option<Color32>) {
  let s = r.width() / 32.0;
  let m = |x: f32, y: f32| r.min + vec2(x * s, y * s);
  let board = Rect::from_min_max(m(3.0, 7.0), m(29.0, 25.0));
  match mono {
    Some(c) => {
      p.rect_stroke(board, 2.0 * s, Stroke::new(1.5 * s, c), egui::StrokeKind::Inside);
      p.rect_filled(Rect::from_min_max(m(12.0, 12.0), m(20.0, 20.0)), 1.0, c);
    }
    None => {
      p.rect_filled(board, 2.0 * s, BOARD_GREEN);
      p.rect_filled(Rect::from_min_max(m(12.0, 12.0), m(20.0, 20.0)), 1.0, Color32::from_gray(40));
      for i in 0..3 {
        let y = 13.5 + i as f32 * 2.5;
        p.line_segment([m(9.0, y), m(12.0, y)], Stroke::new(1.0 * s, Color32::from_rgb(220, 190, 90)));
        p.line_segment([m(20.0, y), m(23.0, y)], Stroke::new(1.0 * s, Color32::from_rgb(220, 190, 90)));
      }
      for (x, y) in [(6.0, 10.0), (26.0, 10.0), (6.0, 22.0), (26.0, 22.0)] {
        p.circle_filled(m(x, y), 1.4 * s, Color32::WHITE);
      }
    }
  }
}

/// A schematic sheet with a resistor and its wires, in `r`.
fn sheet_picture(p: &egui::Painter, r: Rect, mono: Option<Color32>) {
  let s = r.width() / 32.0;
  let m = |x: f32, y: f32| r.min + vec2(x * s, y * s);
  let ink = mono.unwrap_or(theme::ICON_INK);
  let wire = mono.unwrap_or(BOARD_GREEN);
  let page = Rect::from_min_max(m(5.0, 3.0), m(27.0, 29.0));
  if mono.is_none() {
    p.rect_filled(page, 1.0, Color32::WHITE);
  }
  p.rect_stroke(page, 1.0, Stroke::new(1.2 * s, ink), egui::StrokeKind::Inside);
  zigzag(p, m(10.0, 16.0), m(22.0, 16.0), s, ink);
  p.line_segment([m(8.0, 16.0), m(10.0, 16.0)], Stroke::new(1.2 * s, wire));
  p.line_segment([m(22.0, 16.0), m(24.0, 16.0)], Stroke::new(1.2 * s, wire));
  p.line_segment([m(8.0, 16.0), m(8.0, 24.0)], Stroke::new(1.2 * s, wire));
  p.line_segment([m(8.0, 24.0), m(24.0, 24.0)], Stroke::new(1.2 * s, wire));
}

/// A resistor's zigzag from `a` to `b` (horizontal).
fn zigzag(p: &egui::Painter, a: Pos2, b: Pos2, s: f32, c: Color32) {
  let n = 6;
  let pts: Vec<Pos2> = (0..=n)
    .map(|i| {
      let t = i as f32 / n as f32;
      let y = if i == 0 || i == n {
        0.0
      } else if i % 2 == 1 {
        -3.0
      } else {
        3.0
      };
      pos2(a.x + (b.x - a.x) * t, a.y + y * s)
    })
    .collect();
  p.add(egui::Shape::line(pts, Stroke::new(1.4 * s, c)));
}

/// The small link arrow of the Reference commands.
fn link_badge(p: &egui::Painter, r: Rect, mono: Option<Color32>) {
  let s = r.width() / 32.0;
  let c = r.min + vec2(24.0 * s, 24.0 * s);
  p.circle(c, 6.0 * s, Color32::WHITE, Stroke::new(1.0 * s, mono.unwrap_or(theme::ICON_BLUE)));
  let ink = Stroke::new(1.5 * s, mono.unwrap_or(theme::ICON_BLUE));
  p.line_segment([c + vec2(-3.0, 3.0) * s, c + vec2(3.0, -3.0) * s], ink);
  p.line_segment([c + vec2(0.0, -3.0) * s, c + vec2(3.0, -3.0) * s], ink);
  p.line_segment([c + vec2(3.0, -3.0) * s, c + vec2(3.0, 0.0) * s], ink);
}

/// Draws a ribbon icon in `r`: in colour, or all in `mono` (greyed, or white on blue).
pub(crate) fn draw_glyph(p: &egui::Painter, r: Rect, g: Glyph, mono: Option<Color32>) {
  let s = r.width() / 32.0;
  let m = |x: f32, y: f32| r.min + vec2(x * s, y * s);
  let ink = mono.unwrap_or(theme::ICON_INK);
  let green = mono.unwrap_or(BOARD_GREEN);
  let red = mono.unwrap_or(COPPER);
  let blue = mono.unwrap_or(theme::ICON_BLUE);
  let st = |w: f32, c: Color32| Stroke::new(w * s, c);
  match g {
    Glyph::Std(i) => match mono {
      Some(c) => icons::draw(p, r, i, c),
      None => icons::draw_color(p, r, i),
    },
    Glyph::Pcb => board_picture(p, r, mono),
    Glyph::Schematic => sheet_picture(p, r, mono),
    Glyph::RefPcb => {
      board_picture(p, r, mono);
      link_badge(p, r, mono);
    }
    Glyph::RefSchematic => {
      sheet_picture(p, r, mono);
      link_badge(p, r, mono);
    }
    Glyph::FromDesign => {
      let half = Rect::from_min_size(r.min, r.size() * 0.6);
      match mono {
        Some(c) => icons::draw(p, half, Icon::Body, c),
        None => icons::draw_color(p, half, Icon::Body),
      }
      board_picture(p, Rect::from_min_size(r.min + r.size() * 0.4, r.size() * 0.6), mono);
    }
    Glyph::Pcb3d => {
      let top = vec![m(4.0, 14.0), m(18.0, 8.0), m(28.0, 14.0), m(14.0, 20.0)];
      let side = vec![m(4.0, 14.0), m(14.0, 20.0), m(14.0, 23.0), m(4.0, 17.0)];
      let front = vec![m(14.0, 20.0), m(28.0, 14.0), m(28.0, 17.0), m(14.0, 23.0)];
      let (fill, dark) = match mono {
        Some(c) => (c, c),
        None => (BOARD_GREEN, Color32::from_rgb(30, 100, 50)),
      };
      p.add(egui::Shape::convex_polygon(top, fill, Stroke::NONE));
      p.add(egui::Shape::convex_polygon(side, dark, Stroke::NONE));
      p.add(egui::Shape::convex_polygon(front, dark, Stroke::NONE));
      p.add(egui::Shape::convex_polygon(
        vec![m(13.0, 13.0), m(18.0, 11.0), m(21.0, 13.0), m(16.0, 15.0)],
        mono.map_or(Color32::from_gray(40), |_| Color32::WHITE),
        Stroke::NONE,
      ));
    }
    Glyph::Part => {
      zigzag(p, m(9.0, 16.0), m(23.0, 16.0), s, ink);
      p.line_segment([m(3.0, 16.0), m(9.0, 16.0)], st(1.4, green));
      p.line_segment([m(23.0, 16.0), m(29.0, 16.0)], st(1.4, green));
    }
    Glyph::Net => {
      p.add(egui::Shape::line(vec![m(4.0, 8.0), m(16.0, 8.0), m(16.0, 24.0), m(28.0, 24.0)], st(2.0, green)));
      p.circle_filled(m(16.0, 16.0), 2.6 * s, green);
      p.line_segment([m(16.0, 16.0), m(26.0, 16.0)], st(2.0, green));
    }
    Glyph::Bus => {
      p.line_segment([m(4.0, 10.0), m(28.0, 10.0)], st(4.0, blue));
      for x in [10.0, 18.0] {
        p.line_segment([m(x, 10.0), m(x + 4.0, 14.0)], st(1.6, green));
        p.line_segment([m(x + 4.0, 14.0), m(x + 4.0, 26.0)], st(1.6, green));
      }
    }
    Glyph::NetLabel => {
      p.add(egui::Shape::closed_line(
        vec![m(4.0, 11.0), m(22.0, 11.0), m(28.0, 16.0), m(22.0, 21.0), m(4.0, 21.0)],
        st(1.4, ink),
      ));
      p.text(m(13.0, 16.0), egui::Align2::CENTER_CENTER, "N1", egui::FontId::proportional(8.0 * s), blue);
      p.line_segment([m(4.0, 26.0), m(28.0, 26.0)], st(1.6, green));
    }
    Glyph::NoConnect => {
      p.line_segment([m(4.0, 16.0), m(18.0, 16.0)], st(1.6, ink));
      p.line_segment([m(14.0, 10.0), m(26.0, 22.0)], st(2.2, mono.unwrap_or(theme::ICON_RED)));
      p.line_segment([m(14.0, 22.0), m(26.0, 10.0)], st(2.2, mono.unwrap_or(theme::ICON_RED)));
    }
    Glyph::Check => {
      sheet_picture(p, Rect::from_min_size(r.min, r.size() * 0.8), mono);
      p.add(egui::Shape::line(vec![m(16.0, 22.0), m(21.0, 27.0), m(30.0, 15.0)], st(3.0, green)));
    }
    Glyph::Simulate => {
      p.line_segment([m(4.0, 16.0), m(28.0, 16.0)], st(1.0, ink));
      let pts: Vec<Pos2> = (0..=24)
        .map(|i| {
          let t = i as f32 / 24.0;
          m(4.0 + 24.0 * t, 16.0 - 8.0 * (t * std::f32::consts::TAU).sin())
        })
        .collect();
      p.add(egui::Shape::line(pts, st(1.8, blue)));
    }
    Glyph::Layers => {
      for (i, c) in [mono.unwrap_or(Color32::from_rgb(60, 100, 210)), mono.unwrap_or(Color32::from_gray(150)), red]
        .into_iter()
        .enumerate()
      {
        let y = 22.0 - i as f32 * 5.0;
        p.add(egui::Shape::convex_polygon(
          vec![m(16.0, y - 5.0), m(28.0, y), m(16.0, y + 5.0), m(4.0, y)],
          c,
          Stroke::new(1.0 * s, Color32::WHITE),
        ));
      }
    }
    Glyph::Route | Glyph::Autoroute => {
      p.add(egui::Shape::line(vec![m(5.0, 25.0), m(13.0, 25.0), m(21.0, 17.0), m(21.0, 7.0)], st(3.0, red)));
      p.circle_filled(m(5.0, 25.0), 3.0 * s, red);
      p.circle_filled(m(21.0, 7.0), 3.0 * s, red);
      if g == Glyph::Autoroute {
        p.text(m(27.0, 25.0), egui::Align2::CENTER_CENTER, "A", egui::FontId::proportional(11.0 * s), blue);
      }
    }
    Glyph::Via => {
      p.circle_filled(m(16.0, 16.0), 9.0 * s, mono.unwrap_or(Color32::from_rgb(200, 160, 60)));
      p.circle_filled(m(16.0, 16.0), 4.0 * s, Color32::WHITE);
    }
    Glyph::Rotate => {
      let pts: Vec<Pos2> = (0..=20)
        .map(|i| {
          let a = std::f32::consts::PI * (0.15 + 1.5 * i as f32 / 20.0);
          m(16.0 + 9.0 * a.cos(), 16.0 + 9.0 * a.sin())
        })
        .collect();
      let tip = *pts.last().unwrap_or(&m(16.0, 7.0));
      p.add(egui::Shape::line(pts, st(2.0, blue)));
      p.add(egui::Shape::convex_polygon(
        vec![tip + vec2(-4.0, -4.0) * s, tip + vec2(4.0, 0.0) * s, tip + vec2(-4.0, 4.0) * s],
        blue,
        Stroke::NONE,
      ));
    }
    Glyph::Shortcut => {
      p.rect_stroke(Rect::from_min_max(m(5.0, 8.0), m(27.0, 24.0)), 3.0 * s, st(1.4, ink), egui::StrokeKind::Inside);
      p.text(m(16.0, 16.0), egui::Align2::CENTER_CENTER, "/", egui::FontId::proportional(13.0 * s), ink);
    }
    Glyph::Text => {
      p.text(m(16.0, 16.0), egui::Align2::CENTER_CENTER, "T", egui::FontId::proportional(22.0 * s), ink);
    }
    Glyph::Info => {
      p.circle_stroke(m(16.0, 16.0), 10.0 * s, st(1.5, ink));
      p.text(m(16.0, 16.5), egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(13.0 * s), ink);
    }
    Glyph::Stop => {
      let pts: Vec<Pos2> = (0..8)
        .map(|i| {
          let a = std::f32::consts::PI * (0.125 + i as f32 / 4.0);
          m(16.0 + 11.0 * a.cos(), 16.0 + 11.0 * a.sin())
        })
        .collect();
      p.add(egui::Shape::convex_polygon(pts, mono.unwrap_or(theme::ICON_RED), Stroke::NONE));
      p.rect_filled(Rect::from_center_size(m(16.0, 16.0), Vec2::splat(8.0 * s)), 0.0, Color32::WHITE);
    }
    Glyph::ZoomIn | Glyph::ZoomOut => {
      p.circle_stroke(m(14.0, 14.0), 8.0 * s, st(1.8, ink));
      p.line_segment([m(20.0, 20.0), m(27.0, 27.0)], st(2.6, ink));
      p.line_segment([m(10.0, 14.0), m(18.0, 14.0)], st(1.8, ink));
      if g == Glyph::ZoomIn {
        p.line_segment([m(14.0, 10.0), m(14.0, 18.0)], st(1.8, ink));
      }
    }
  }
}

/// A tab's document icon: the component for a design, Fusion's green board for an electronics
/// design or board, a sheet for a schematic.
pub(crate) fn doc_icon(p: &egui::Painter, r: Rect, kind: DocKind) {
  match kind {
    DocKind::Design => icons::draw_color(p, r, Icon::Component),
    DocKind::Electronics | DocKind::Board => board_picture(p, r, None),
    DocKind::Schematic => sheet_picture(p, r, None),
  }
}

// ---------------------------------------------------------------- the ribbon

/// A ribbon command: its icon and name; greyed when FissionOxide can't do it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cmd {
  pub glyph: Glyph,
  pub label: &'static str,
  pub on: bool,
}

const fn c(glyph: Glyph, label: &'static str) -> Cmd {
  Cmd { glyph, label, on: true }
}

const fn x(glyph: Glyph, label: &'static str) -> Cmd {
  Cmd { glyph, label, on: false }
}

pub const NEW_SCHEMATIC: &str = "New Schematic";
pub const REF_SCHEMATIC: &str = "Reference a Schematic document";
pub const NEW_PCB: &str = "New PCB";
pub const REF_PCB: &str = "Reference a PCB document";
pub const FROM_DESIGN: &str = "From Design";
pub const IMPORT_FILE: &str = "Import Non-Fusion File";
pub const LEARNING: &str = "Learning Resources";
const OUTPUTS: &str = "Manufacturing Outputs";
const SCRIPTS: &str = "Scripts and Add-Ins";
const MARKETPLACE: &str = "Design and Make Marketplace";
pub const TO_PCB: &str = "Switch to PCB document";
pub const TO_SCHEMATIC: &str = "Switch to Schematic document";
const PUSH_3D: &str = "Push to 3D PCB";
pub const FIT: &str = "Zoom to Fit";
pub const GRID: &str = "Grid";
const ZOOM_IN: &str = "Zoom In";
const ZOOM_OUT: &str = "Zoom Out";
pub const MOVE: &str = "Move";
pub const DELETE: &str = "Delete";
const UNDO: &str = "Undo";
const REDO: &str = "Redo";
const LAYER_SETTINGS: &str = "Layer Settings";
pub const PLACE: &str = "Place Component";
const TEXT: &str = "Text";
pub const NET: &str = "Net";
const BUS: &str = "Bus";
pub const NET_LABEL: &str = "Net Label";
pub const NO_CONNECT: &str = "No Connect";
const SIMULATE: &str = "Simulate";
const REWORK: &str = "Rework";
pub const ROTATE: &str = "Rotate";
pub const MIRROR: &str = "Mirror";
const FLIP: &str = "Flip to Other Side";
pub const COMMAND_LINE: &str = "Command Line";
pub const SELECT: &str = "Select";
pub const ERC: &str = "Electrical Rules Check";
pub const DRC: &str = "Design Rules Check";
const IMPORT_LIBRARY: &str = "Import Library";
pub const EXAMPLE: &str = "Open the Example Board";
const BOARD_SHAPE: &str = "Board Shape";
pub const ROUTE: &str = "Route";
pub const VIA: &str = "Via";
const AUTOROUTE: &str = "Autoroute";
const FAB: &str = "Manufacturing Files";
const DOC_SETTINGS: &str = "Document Settings";
const NAMED_VIEWS: &str = "Named Views";
const INFO: &str = "Info";
const VISIBILITY: &str = "Visibility";
const STOP: &str = "Stop";

/// The ribbon's panels for what the tab shows: (name, commands).
pub fn ribbon_panels(v: &EcadView) -> Vec<(&'static str, Vec<Cmd>)> {
  use Glyph::*;
  let board = v.sheet == Sheet::Board;
  match (v.page, v.ribbon) {
    (Page::Start, 0) => vec![
      (
        "CREATE",
        vec![
          c(Schematic, NEW_SCHEMATIC),
          x(RefSchematic, REF_SCHEMATIC),
          c(Pcb, NEW_PCB),
          x(RefPcb, REF_PCB),
          x(FromDesign, FROM_DESIGN),
        ],
      ),
      ("IMPORT", vec![x(Std(Icon::Import), IMPORT_FILE)]),
      ("OUTPUTS", vec![x(Std(Icon::ExportPdf), OUTPUTS)]),
    ],
    (Page::Start, _) => vec![("ADD-INS", vec![c(Std(Icon::Scripts), SCRIPTS), x(Std(Icon::Marketplace), MARKETPLACE)])],
    (Page::Editor, 0) if board => vec![
      ("SWITCH", vec![c(Schematic, TO_SCHEMATIC), c(Pcb3d, PUSH_3D)]),
      ("VIEW", vec![c(Std(Icon::Fit), FIT), c(Std(Icon::Grid), GRID)]),
      (
        "EDIT",
        vec![
          c(Std(Icon::Move), MOVE),
          c(Std(Icon::Remove), DELETE),
          c(Std(Icon::Undo), UNDO),
          c(Std(Icon::Redo), REDO),
        ],
      ),
      ("LAYERS", vec![x(Layers, LAYER_SETTINGS)]),
      ("BOARD", vec![c(Std(Icon::Rect), BOARD_SHAPE)]),
      ("ROUTE", vec![c(Route, ROUTE), c(Via, VIA), c(Autoroute, AUTOROUTE)]),
      ("MODIFY", vec![c(Rotate, ROTATE), c(Std(Icon::Mirror), FLIP)]),
      ("SHORTCUTS", vec![c(Shortcut, COMMAND_LINE)]),
      ("SELECT", vec![c(Std(Icon::Select), SELECT)]),
    ],
    (Page::Editor, 0) => vec![
      ("SWITCH", vec![c(Pcb, TO_PCB)]),
      ("VIEW", vec![c(Std(Icon::Fit), FIT), c(Std(Icon::Grid), GRID)]),
      (
        "EDIT",
        vec![
          c(Std(Icon::Move), MOVE),
          c(Std(Icon::Remove), DELETE),
          c(Std(Icon::Undo), UNDO),
          c(Std(Icon::Redo), REDO),
        ],
      ),
      ("LAYERS", vec![x(Layers, LAYER_SETTINGS)]),
      ("PLACE", vec![c(Part, PLACE), x(Text, TEXT)]),
      ("CONNECT", vec![c(Net, NET), x(Bus, BUS), c(NetLabel, NET_LABEL), c(NoConnect, NO_CONNECT)]),
      ("SIMULATE", vec![x(Simulate, SIMULATE)]),
      ("REWORK", vec![x(Std(Icon::Repair), REWORK)]),
      ("MODIFY", vec![c(Rotate, ROTATE), c(Std(Icon::Mirror), MIRROR)]),
      ("SHORTCUTS", vec![c(Shortcut, COMMAND_LINE)]),
      ("SELECT", vec![c(Std(Icon::Select), SELECT)]),
    ],
    (Page::Editor, 1) => vec![("DOCUMENT", vec![x(Std(Icon::Gear), DOC_SETTINGS), x(Std(Icon::Views), NAMED_VIEWS)])],
    (Page::Editor, 2) => vec![("VALIDATE", vec![if board { c(Check, DRC) } else { c(Check, ERC) }])],
    (Page::Editor, 3) => vec![("LIBRARY", vec![c(Std(Icon::Import), IMPORT_LIBRARY)])],
    (Page::Editor, _) => {
      let mut v = vec![("EXAMPLES", vec![c(Std(Icon::Folder), EXAMPLE)])];
      if board {
        v.push(("OUTPUTS", vec![c(Std(Icon::ExportPdf), FAB)]));
      }
      v.push(("ADD-INS", vec![c(Std(Icon::Scripts), SCRIPTS)]));
      v
    }
  }
}

/// Whether a command shows as running (white on blue).
fn is_active(v: &EcadView, label: &str) -> bool {
  match label {
    SELECT => v.tool == Tool::Select,
    NET => v.tool == Tool::Wire,
    NET_LABEL => v.tool == Tool::Label,
    NO_CONNECT => v.tool == Tool::NoConnect,
    DELETE => v.tool == Tool::Delete,
    ROUTE => v.tool == Tool::Route,
    VIA => v.tool == Tool::Via,
    PLACE => matches!(v.tool, Tool::Place(_)),
    _ => false,
  }
}

/// A greyed command's tooltip.
pub(crate) fn unavailable(label: &str) -> String {
  format!("{label} isn't in FissionOxide yet")
}

/// The ribbon: the page's tabs, then the shown tab's panels.
pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
  let Some(mut v) = app.electronics.take() else { return };
  let tabs: &[&str] = if v.page == Page::Start { &START_TABS } else { &EDITOR_TABS };
  v.ribbon = v.ribbon.min(tabs.len() - 1);
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 4.0;
    for (i, t) in tabs.iter().enumerate() {
      if panels::ribbon_tab(ui, t, v.ribbon == i).clicked() {
        v.ribbon = i;
      }
    }
  });
  let mut hit = None;
  egui::ScrollArea::horizontal()
    .id_salt("ecad-ribbon")
    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
    .show(ui, |ui| {
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (title, cmds) in ribbon_panels(&v) {
          let popup_id = egui::Id::new(("ecad-panel", title));
          let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
          let (picked, label) = panel_body(ui, &v, title, &cmds, open);
          hit = hit.or(picked);
          if title == "BOARD" {
            // the outline from a sketch, a face or a size; copper layers; pours
            egui::Popup::menu(&label).id(popup_id).show(|ui| ecad_ui::board_menu(app, ui, &mut v));
          } else {
            egui::Popup::menu(&label).id(popup_id).show(|ui| {
              ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
              for cmd in &cmds {
                if menu_row(ui, cmd).clicked() && cmd.on {
                  hit = Some(cmd.label);
                  ui.close();
                }
              }
            });
          }
          panels::separator(ui);
        }
      })
    });
  app.electronics = Some(v);
  match hit {
    // the BOARD panel's menu holds the choices
    Some(BOARD_SHAPE) => egui::Popup::open_id(ui.ctx(), egui::Id::new(("ecad-panel", "BOARD"))),
    Some(label) => run_cmd(app, label),
    None => {}
  }
}

/// A panel's icons and its name under them; returns the command clicked and the name's response.
fn panel_body(
  ui: &mut egui::Ui,
  v: &EcadView,
  title: &str,
  cmds: &[Cmd],
  open: bool,
) -> (Option<&'static str>, egui::Response) {
  let mut hit = None;
  let inner = ui.vertical(|ui| {
    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
      for cmd in cmds {
        let (rect, resp) = ui.allocate_exact_size(vec2(icons::TOOL_BUTTON_W, 40.0), Sense::click());
        let ic = Rect::from_center_size(rect.center(), Vec2::splat(32.0));
        if is_active(v, cmd.label) {
          ui.painter().rect_filled(rect, 2.0, theme::ACCENT);
          draw_glyph(ui.painter(), ic, cmd.glyph, Some(Color32::WHITE));
        } else {
          if resp.hovered() && cmd.on {
            ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
          }
          draw_glyph(ui.painter(), ic, cmd.glyph, (!cmd.on).then_some(GREYED));
        }
        let resp = if cmd.on { resp.on_hover_text(cmd.label) } else { resp.on_hover_text(unavailable(cmd.label)) };
        if resp.clicked() && cmd.on {
          hit = Some(cmd.label);
        }
      }
    });
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    panels::group_label(ui, title, w, open)
  });
  (hit, inner.inner)
}

/// A row of a panel's menu: the icon and the name, greyed when not available.
fn menu_row(ui: &mut egui::Ui, cmd: &Cmd) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(250.0, 26.0), Sense::click());
  if resp.hovered() && cmd.on {
    ui.painter().rect_filled(rect, 0.0, theme::HOVER);
  }
  let ic = Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), Vec2::splat(18.0));
  draw_glyph(ui.painter(), ic, cmd.glyph, (!cmd.on).then_some(GREYED));
  ui.painter().text(
    rect.left_center() + vec2(34.0, 0.0),
    egui::Align2::LEFT_CENTER,
    cmd.label,
    egui::FontId::proportional(12.5),
    if cmd.on { theme::TEXT } else { theme::MUTED },
  );
  if cmd.on { resp } else { resp.on_hover_text(unavailable(cmd.label)) }
}

// ---------------------------------------------------------------- commands

/// Runs a ribbon command (or a start page button) on the shown electronics document.
pub(crate) fn run_cmd(app: &mut App, label: &str) {
  match label {
    NEW_SCHEMATIC => return new_editor(app, Sheet::Schematic),
    NEW_PCB => return new_editor(app, Sheet::Board),
    SCRIPTS => return crate::plugins_ui::open_window(app),
    LEARNING => return crate::help::open(app, crate::help::DOCS),
    _ => {}
  }
  let Some(mut v) = app.electronics.take() else { return };
  edit_cmd(app, &mut v, label);
  app.electronics = Some(v);
}

fn set_tool(v: &mut EcadView, t: Tool) {
  v.tool = t;
  v.pending.clear();
}

fn set_sheet(v: &mut EcadView, s: Sheet) {
  v.sheet = s;
  v.selected = None;
  set_tool(v, Tool::Select);
}

/// Zooms the shown sheet about its centre.
fn zoom_by(v: &mut EcadView, f: f32) {
  let k = if v.sheet == Sheet::Schematic { 0 } else { 1 };
  let (z, pan) = v.cam[k];
  let new = (z * f).clamp(0.3, 400.0);
  v.cam[k] = (new, pan * (new / z));
}

/// The editor's commands, with the document's editor state out of the app.
pub(crate) fn edit_cmd(app: &mut App, v: &mut EcadView, label: &str) {
  let selected = v.selected.clone();
  let pr = app.s.design.electronics.clone();
  match label {
    TO_PCB => set_sheet(v, Sheet::Board),
    TO_SCHEMATIC => set_sheet(v, Sheet::Schematic),
    FIT => v.fitted[if v.sheet == Sheet::Schematic { 0 } else { 1 }] = false,
    GRID => v.grid = !v.grid,
    ZOOM_IN => zoom_by(v, 1.25),
    ZOOM_OUT => zoom_by(v, 0.8),
    SELECT | MOVE => set_tool(v, Tool::Select),
    DELETE => set_tool(v, Tool::Delete),
    UNDO => {
      app.s.undo();
    }
    REDO => {
      app.s.redo();
    }
    PLACE => {
      v.place_open = true;
      v.place_tab = 0;
      set_sheet(v, Sheet::Schematic);
    }
    NET => set_tool(v, Tool::Wire),
    NET_LABEL => set_tool(v, Tool::Label),
    NO_CONNECT => set_tool(v, Tool::NoConnect),
    ROUTE => set_tool(v, Tool::Route),
    VIA => set_tool(v, Tool::Via),
    ROTATE | MIRROR | FLIP => {
      let Some(sel) = selected else {
        app.status = Some((format!("{label}: select a part first"), false));
        return;
      };
      let pr = pr.as_deref();
      match (v.sheet, label) {
        (Sheet::Schematic, ROTATE) => {
          let r = pr.and_then(|p| p.schematic.parts.iter().find(|x| x.refdes == sel)).map_or(0.0, |x| x.rot);
          app.run("ecad.part.edit", json!({"refdes": sel, "rot": r + 90.0}));
        }
        (Sheet::Schematic, _) => {
          let m = pr.and_then(|p| p.schematic.parts.iter().find(|x| x.refdes == sel)).is_some_and(|x| x.mirror);
          app.run("ecad.part.edit", json!({"refdes": sel, "mirror": !m}));
        }
        (Sheet::Board, ROTATE) => {
          let r = pr.and_then(|p| p.board.placement(&sel)).map_or(0.0, |x| x.rot);
          app.run("ecad.place", json!({"refdes": sel, "rot": r + 90.0}));
        }
        (Sheet::Board, _) => {
          let bottom = pr.and_then(|p| p.board.placement(&sel)).is_some_and(|x| x.side == Side::Bottom);
          app.run("ecad.place", json!({"refdes": sel, "side": if bottom { "top" } else { "bottom" }}));
        }
      }
    }
    COMMAND_LINE => v.focus_cmdline = true,
    ERC | DRC => {
      let (cmd, title) =
        if label == ERC { ("ecad.erc", "Electrical rules check") } else { ("ecad.drc", "Design rules check") };
      if pr.is_none() {
        v.report = Some((title.into(), Vec::new()));
      } else if let Some(r) = app.run(cmd, json!({})) {
        v.report = Some((title.into(), serde_json::from_value(r["violations"].clone()).unwrap_or_default()));
      }
    }
    IMPORT_LIBRARY => {
      if let Some(p) = rfd::FileDialog::new().add_filter("Libraries", &["kicad_sym", "kicad_mod", "json"]).pick_file()
        && let Some(r) = app.run("ecad.library.import", json!({"path": p}))
      {
        app.status = Some((format!("Imported {} devices, {} footprints", r["devices"], r["footprints"]), false));
      }
    }
    EXAMPLE => {
      app.run("ecad.new", json!({"example": true}));
      v.fitted = [false; 2];
    }
    AUTOROUTE => {
      if let Some(r) = app.run("ecad.autoroute", json!({})) {
        app.status = Some((format!("Routed {} connections; {} left", r["routed"], r["unrouted"]), false));
      }
    }
    PUSH_3D => {
      if let Some(r) = app.run("ecad.push3d", json!({})) {
        app.status = Some((
          format!("PCB feature {} with {} bodies", r["feature"], r["bodies"].as_array().map_or(0, |a| a.len())),
          false,
        ));
      }
    }
    FAB => {
      if let Some(dir) = rfd::FileDialog::new().set_title("Folder for the fab files").pick_folder()
        && let Some(r) = app.run("ecad.export", json!({"dir": dir}))
      {
        let n = r["files"].as_array().map_or(0, |a| a.len());
        let msg = if r["drc_errors"].as_u64().unwrap_or(0) > 0 {
          format!("Wrote {n} files to {} — the board still has DRC errors", dir.display())
        } else {
          format!("Wrote {n} files to {}", dir.display())
        };
        app.status = Some((msg, false));
      }
    }
    STOP => {
      if v.pending.is_empty() {
        set_tool(v, Tool::Select);
      } else {
        ecad_ui::finish(app, v);
      }
    }
    INFO => {
      let msg = match &pr {
        Some(pr) => format!(
          "{}: {} parts, {} nets, {} tracks, {} vias",
          pr.name,
          pr.schematic.parts.iter().filter(|p| !p.refdes.starts_with('#')).count(),
          pr.netlist().nets.len(),
          pr.board.tracks.len(),
          pr.board.vias.len()
        ),
        None => "The sheet is empty".to_string(),
      };
      app.status = Some((msg, false));
    }
    other => app.status = Some((unavailable(other), false)),
  }
}

/// The command line's id (/ focuses it).
pub(crate) fn cmdline_id() -> egui::Id {
  egui::Id::new("ecad-cmdline")
}

/// Runs a line typed in the command line, EAGLE's words: ADD part, NET (or WIRE), LABEL name,
/// MOVE, DELETE, ROTATE, MIRROR, ROUTE, VIA, ERC, DRC, GRID, WINDOW FIT, BOARD, SCHEMATIC, UNDO,
/// REDO.
pub(crate) fn command_line(app: &mut App, v: &mut EcadView, line: &str) {
  let line = line.trim().trim_start_matches('/').trim();
  let mut words = line.split_whitespace();
  let Some(word) = words.next() else { return };
  let arg = words.collect::<Vec<_>>().join(" ");
  let label = match word.to_ascii_uppercase().as_str() {
    "ADD" => {
      let list = app.s.execute("ecad.library", &json!({"query": arg})).unwrap_or(Value::Null);
      let names: Vec<String> =
        list.as_array().into_iter().flatten().filter_map(|d| d["device"].as_str().map(str::to_string)).collect();
      match names.iter().find(|n| n.eq_ignore_ascii_case(&arg)).or(names.first()) {
        Some(n) if !arg.is_empty() => {
          set_sheet(v, Sheet::Schematic);
          set_tool(v, Tool::Place(n.clone()));
        }
        _ => {
          v.place_open = true;
          v.query = arg;
        }
      }
      return;
    }
    "LABEL" => {
      if !arg.is_empty() {
        v.label = arg;
      }
      NET_LABEL
    }
    "NET" | "WIRE" => NET,
    "MOVE" => MOVE,
    "SELECT" | "GROUP" => SELECT,
    "DELETE" => DELETE,
    "ROTATE" => ROTATE,
    "MIRROR" => {
      if v.sheet == Sheet::Board {
        FLIP
      } else {
        MIRROR
      }
    }
    "ROUTE" => ROUTE,
    "VIA" => VIA,
    "ERC" => ERC,
    "DRC" => DRC,
    "GRID" => GRID,
    "WINDOW" | "FIT" => FIT,
    "BOARD" => TO_PCB,
    "SCHEMATIC" | "SCH" => TO_SCHEMATIC,
    "UNDO" => UNDO,
    "REDO" => REDO,
    "AUTO" | "AUTOROUTER" => AUTOROUTE,
    "BUS" => BUS,
    other => {
      app.status = Some((format!("Unknown command: {other}"), true));
      return;
    }
  };
  edit_cmd(app, v, label);
}

/// A TEXT COMMANDS line: in Txt, a FissionOxide command and its JSON parameters
/// (`ecad.part.add {"device": "R_0603", "at": [0, 0]}`); Python and TypeScript aren't in
/// FissionOxide.
pub(crate) fn text_command(app: &mut App, v: &mut EcadView, line: &str) {
  let line = line.trim();
  if line.is_empty() {
    return;
  }
  v.text_log.push(format!("> {line}"));
  if v.text_mode != TextMode::Txt {
    let lang = if v.text_mode == TextMode::Py { "Python" } else { "TypeScript" };
    v.text_log.push(format!("{lang} commands aren't in FissionOxide yet"));
    return;
  }
  let (name, params) = match line.split_once(char::is_whitespace) {
    Some((n, rest)) => (n, rest.trim()),
    None => (line, ""),
  };
  let params: Value = if params.is_empty() {
    json!({})
  } else {
    match serde_json::from_str(params) {
      Ok(p) => p,
      Err(e) => {
        v.text_log.push(format!("Error: the parameters aren't JSON ({e})"));
        return;
      }
    }
  };
  let out = match app.s.execute(name, &params) {
    Ok(r) => r.to_string(),
    Err(e) => format!("Error: {e}"),
  };
  v.text_log.push(out);
}

// ---------------------------------------------------------------- the start page

/// The electronics design's page: the browser and Description at the left, the start page in the
/// canvas and TEXT COMMANDS at the bottom.
pub(crate) fn start_page(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView) {
  let full = ui.available_rect_before_wrap();
  let bottom_h = text_commands_height(v);
  let canvas = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - bottom_h));
  ui.allocate_rect(canvas, Sense::hover());
  app.canvas = canvas;
  ui.painter().rect_filled(canvas, 0.0, Color32::WHITE);
  // the centre: heading, the board, "Start with a schematic" and the buttons
  let mut act = None;
  let col = Rect::from_center_size(canvas.center(), vec2(360.0, 400.0_f32.min(canvas.height())));
  ui.scope_builder(egui::UiBuilder::new().max_rect(col), |ui| {
    ui.vertical_centered(|ui| {
      ui.spacing_mut().item_spacing.y = 10.0;
      ui.label(RichText::new(START_HEADING).size(22.0).color(theme::TEXT));
      let (pic, _) = ui.allocate_exact_size(vec2(240.0, 150.0), Sense::hover());
      start_illustration(ui.painter(), pic);
      ui.label(RichText::new(START_LEAD).size(14.0).color(theme::MUTED));
      ui.add_space(4.0);
      if start_button(ui, NEW_SCHEMATIC, true, true).clicked() {
        act = Some(NEW_SCHEMATIC);
      }
      start_button(ui, IMPORT_FILE, false, false);
      if start_button(ui, LEARNING, false, true).clicked() {
        act = Some(LEARNING);
      }
    });
  });
  start_browser(app, ui, v, canvas);
  let bottom = Rect::from_min_max(pos2(full.min.x, canvas.max.y), full.max);
  ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| text_commands(app, ui, v, bottom));
  if let Some(a) = act {
    // the new tab parks this document: its state goes back first
    app.electronics = Some(std::mem::take(v));
    run_cmd(app, a);
    if let Some(back) = app.electronics.take() {
      *v = back;
    }
  }
}

/// A start page button: Fusion's blue New Schematic, the others outlined; greyed when off.
fn start_button(ui: &mut egui::Ui, label: &str, primary: bool, on: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(220.0, 32.0), if on { Sense::click() } else { Sense::hover() });
  let hovered = resp.hovered() && on;
  let (fill, stroke, text) = if primary {
    (if hovered { theme::ACCENT_DARK } else { theme::ACCENT }, Stroke::NONE, Color32::WHITE)
  } else {
    (
      if hovered { theme::HOVER } else { Color32::WHITE },
      Stroke::new(1.0, if on { Color32::from_gray(150) } else { Color32::from_gray(210) }),
      if on { theme::TEXT } else { theme::MUTED },
    )
  };
  ui.painter().rect(rect, 3.0, fill, stroke, egui::StrokeKind::Inside);
  ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), text);
  if on { resp } else { resp.on_hover_text(unavailable(label)) }
}

/// The start page's picture: a populated circuit board.
fn start_illustration(p: &egui::Painter, r: Rect) {
  let board = r.shrink2(vec2(10.0, 12.0));
  p.rect_filled(board.translate(vec2(4.0, 5.0)), 8.0, Color32::from_rgb(30, 100, 50));
  p.rect_filled(board, 8.0, BOARD_GREEN);
  let gold = Color32::from_rgb(220, 190, 90);
  let at = |x: f32, y: f32| board.min + vec2(x * board.width(), y * board.height());
  // traces
  for (a, b, c) in [
    ((0.12, 0.3), (0.4, 0.3), (0.4, 0.45)),
    ((0.6, 0.55), (0.85, 0.55), (0.85, 0.8)),
    ((0.2, 0.75), (0.45, 0.75), (0.45, 0.62)),
  ] {
    p.add(egui::Shape::line(vec![at(a.0, a.1), at(b.0, b.1), at(c.0, c.1)], Stroke::new(2.0, gold)));
  }
  // the chip with its pins, a few passives and mounting holes
  let chip = Rect::from_min_max(at(0.38, 0.38), at(0.62, 0.68));
  for i in 0..5 {
    let y = chip.top() + 6.0 + i as f32 * (chip.height() - 12.0) / 4.0;
    p.line_segment([pos2(chip.left() - 6.0, y), pos2(chip.left(), y)], Stroke::new(2.0, Color32::from_gray(200)));
    p.line_segment([pos2(chip.right(), y), pos2(chip.right() + 6.0, y)], Stroke::new(2.0, Color32::from_gray(200)));
  }
  p.rect_filled(chip, 2.0, Color32::from_gray(35));
  for (x, y) in [(0.15, 0.55), (0.75, 0.25), (0.75, 0.38)] {
    let r = Rect::from_center_size(at(x, y), vec2(18.0, 8.0));
    p.rect_filled(r, 1.0, Color32::from_rgb(190, 160, 120));
    p.rect_filled(Rect::from_min_size(r.min, vec2(4.0, 8.0)), 0.0, gold);
    p.rect_filled(Rect::from_min_size(r.right_top() - vec2(4.0, 0.0), vec2(4.0, 8.0)), 0.0, gold);
  }
  for (x, y) in [(0.06, 0.1), (0.94, 0.1), (0.06, 0.9), (0.94, 0.9)] {
    p.circle_filled(at(x, y), 5.0, gold);
    p.circle_filled(at(x, y), 2.5, Color32::WHITE);
  }
}

/// The start page's browser: the "(Unsaved)" root alone, then the Description with its Edit
/// button.
fn start_browser(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, canvas: Rect) {
  let width = 270.0;
  let area = Rect::from_min_size(canvas.min + vec2(6.0, 6.0), vec2(width, (canvas.height() - 12.0).max(200.0)));
  ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    panels::float_header(ui, "BROWSER", &mut app.browser_open, width);
    if app.browser_open {
      ui.add_space(4.0);
      let label = if documents::unsaved(&app.s) { "(Unsaved)".to_string() } else { documents::doc_name(&app.s) };
      panels::tree_row(
        ui,
        egui::Id::new("ecad-start-root"),
        0,
        None,
        None,
        Icon::Component,
        &label,
        false,
        theme::TEXT,
      );
      ui.add_space(12.0);
    }
    // Description: the design's notes, edited in place
    let mut open = true;
    panels::float_header(ui, "Description", &mut open, width);
    ui.add_space(4.0);
    let r = Rect::from_min_size(ui.cursor().min, vec2(width, 90.0));
    ui.painter().rect(r, 2.0, Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    ui.scope_builder(egui::UiBuilder::new().max_rect(r.shrink(6.0)), |ui| {
      if v.editing_description {
        ui.add(
          egui::TextEdit::multiline(&mut v.description)
            .desired_width(width - 12.0)
            .desired_rows(4)
            .frame(egui::Frame::NONE),
        );
      } else if v.description.is_empty() {
        ui.label(RichText::new("No description").color(theme::MUTED));
      } else {
        ui.label(RichText::new(&v.description).color(theme::TEXT));
      }
    });
    // the Edit button goes under the box, whatever its text took
    ui.advance_cursor_after_rect(r);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
      let label = if v.editing_description { "Done" } else { "Edit" };
      if ui.button(label).clicked() {
        v.editing_description = !v.editing_description;
      }
    });
  });
}

// ---------------------------------------------------------------- the editor

/// Where the editor's parts go.
pub(crate) struct EditorLayout {
  pub command: Rect,
  pub place: Option<Rect>,
  pub canvas: Rect,
  pub bottom: Rect,
}

/// The command row's height, PLACE COMPONENTS' width.
const COMMAND_H: f32 = 30.0;
const PLACE_W: f32 = 250.0;

impl EditorLayout {
  pub(crate) fn new(full: Rect, v: &EcadView) -> EditorLayout {
    let command = Rect::from_min_size(full.min, vec2(full.width(), COMMAND_H));
    let bottom = Rect::from_min_max(pos2(full.min.x, full.max.y - bottom_height(v)), full.max);
    let body = Rect::from_min_max(pos2(full.min.x, command.max.y), pos2(full.max.x, bottom.min.y));
    let (canvas, place) = if v.place_open {
      let place = Rect::from_min_max(pos2(body.max.x - PLACE_W, body.min.y), body.max);
      (Rect::from_min_max(body.min, pos2(place.min.x, body.max.y)), Some(place))
    } else {
      (body, None)
    };
    EditorLayout { command, place, canvas, bottom }
  }
}

/// The grid readout: Fusion's "0.1 inch (4.2 0.3)", the cursor in inches.
pub(crate) fn grid_readout(v: &EcadView, at: Option<P>) -> String {
  let (grid, unit_mm, unit) = if v.sheet == Sheet::Schematic { (0.1, 25.4, "inch") } else { (0.25, 1.0, "mm") };
  match at {
    Some(p) => format!("{grid} {unit} ({:.1} {:.1})", p[0] / unit_mm, p[1] / unit_mm),
    None => format!("{grid} {unit}"),
  }
}

/// Under the ribbon: the net class, the grid readout and the command line.
pub(crate) fn command_row(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, r: Rect, at: Option<P>) {
  ui.painter().rect_filled(r, 0.0, theme::TOOLBAR);
  ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, theme::BORDER));
  let mut line = None;
  ui.scope_builder(egui::UiBuilder::new().max_rect(r.shrink2(vec2(8.0, 3.0))), |ui| {
    ui.horizontal_centered(|ui| {
      // the net class, with its colour swatch
      let (sw, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
      ui.painter().rect_filled(sw, 2.0, Color32::from_rgb(60, 170, 80));
      egui::ComboBox::from_id_salt("ecad-net-class").width(110.0).selected_text(NET_CLASS).show_ui(ui, |ui| {
        let _ = ui.selectable_label(true, NET_CLASS);
      });
      ui.add_space(8.0);
      ui.label(RichText::new(grid_readout(v, at)).color(theme::TEXT).monospace());
      ui.add_space(8.0);
      // the net label's name while placing labels, as EAGLE's
      if v.tool == Tool::Label {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut v.label).desired_width(70.0));
      }
      if matches!(v.tool, Tool::Route | Tool::Via) {
        let mut layers =
          vec![fox_engine::electronics::board::Layer::Top, fox_engine::electronics::board::Layer::Bottom];
        if app.s.design.electronics.as_ref().is_some_and(|p| p.board.layers >= 4) {
          layers = app.s.design.electronics.as_ref().map(|p| p.board.copper_layers()).unwrap_or(layers);
        }
        egui::ComboBox::from_id_salt("ecad_layer").selected_text(v.layer.name()).width(70.0).show_ui(ui, |ui| {
          for l in layers {
            ui.selectable_value(&mut v.layer, l, l.name());
          }
        });
      }
      let resp = ui.add(
        egui::TextEdit::singleline(&mut v.cmdline)
          .id(cmdline_id())
          .hint_text(CMDLINE_HINT)
          .desired_width(f32::INFINITY),
      );
      if std::mem::take(&mut v.focus_cmdline) {
        resp.request_focus();
      }
      if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        line = Some(std::mem::take(&mut v.cmdline));
      }
    });
  });
  if let Some(l) = line {
    command_line(app, v, &l);
  }
}

/// PLACE COMPONENTS at the right: icon tabs (the libraries, the parts placed lately), the search
/// box and the parts; the selected part's name and value under them.
pub(crate) fn place_panel(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, r: Option<Rect>) {
  let Some(r) = r else { return };
  ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
    ui.painter().rect_filled(r, 0.0, theme::PANEL);
    ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, theme::BORDER));
    ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
    let mut open = v.place_open;
    panels::float_header(ui, "PLACE COMPONENTS", &mut open, r.width());
    v.place_open = open;
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 2.0;
      ui.add_space(6.0);
      for (i, (icon, tip)) in [(Icon::Folder, "Libraries"), (Icon::History, "Recently placed")].into_iter().enumerate()
      {
        if icons::icon_button(ui, icon, 28.0, tip, v.place_tab == i, None).clicked() {
          v.place_tab = i;
        }
      }
    });
    ui.horizontal(|ui| {
      ui.add_space(6.0);
      ui.add(egui::TextEdit::singleline(&mut v.query).hint_text("Search").desired_width(r.width() - 14.0));
    });
    let list = app.s.execute("ecad.library", &json!({"query": v.query})).unwrap_or(Value::Null);
    let mut parts: Vec<&Value> = list.as_array().into_iter().flatten().collect();
    if v.place_tab == 1 {
      parts.retain(|d| d["device"].as_str().is_some_and(|n| v.recent.iter().any(|r| r == n)));
    }
    egui::ScrollArea::vertical().id_salt("ecad_lib").max_height(r.height() * 0.6).show(ui, |ui| {
      ui.spacing_mut().item_spacing.y = 1.0;
      ui.set_width(r.width() - 12.0);
      if parts.is_empty() {
        ui.label(RichText::new("No parts").color(theme::MUTED));
      }
      for d in parts {
        let name = d["device"].as_str().unwrap_or("").to_string();
        let lib = d["library"].as_str().unwrap_or("");
        let on = v.tool == Tool::Place(name.clone());
        let text = RichText::new(format!("{name}\n{}", d["description"].as_str().unwrap_or(""))).size(11.5);
        let resp = ui.add(egui::Button::selectable(on, text)).on_hover_text(format!(
          "{lib}: {} / {}",
          d["symbol"].as_str().unwrap_or(""),
          d["footprint"].as_str().unwrap_or("")
        ));
        if resp.clicked() {
          set_sheet(v, Sheet::Schematic);
          set_tool(v, Tool::Place(name));
        }
      }
    });
    ui.separator();
    // the selected part: its name, device and value
    let mut revalue: Option<(String, String)> = None;
    if let Some(pr) = app.s.design.electronics.as_deref()
      && let Some(sel) = &v.selected
      && let Some(part) = pr.schematic.parts.iter().find(|p| &p.refdes == sel)
    {
      ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.vertical(|ui| {
          ui.label(RichText::new(format!("{} — {}", part.refdes, part.device)).strong());
          let mut value = part.value.clone();
          ui.horizontal(|ui| {
            ui.label("Value");
            if ui.add(egui::TextEdit::singleline(&mut value).desired_width(90.0)).lost_focus() && value != part.value {
              revalue = Some((part.refdes.clone(), value.clone()));
            }
          });
        });
      });
    }
    if let Some((refdes, value)) = revalue {
      app.run("ecad.part.edit", json!({"refdes": refdes, "value": value}));
    }
  });
}

/// The editor's browser over the sheet: the schematic (or PCB) with Document Settings, Named
/// Views and its sheets.
pub(crate) fn editor_browser(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, canvas: Rect) {
  let width = 230.0;
  let area = Rect::from_min_size(canvas.min + vec2(6.0, 6.0), vec2(width, (canvas.height() - 60.0).max(80.0)));
  ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    panels::float_header(ui, "BROWSER", &mut app.browser_open, width);
    if !app.browser_open {
      return;
    }
    ui.add_space(4.0);
    let (root, rows): (&str, &[(Icon, &str)]) = match v.sheet {
      Sheet::Schematic => {
        ("Schematic", &[(Icon::Gear, DOC_SETTINGS), (Icon::Views, NAMED_VIEWS), (Icon::SheetSettings, "Sheet 1/1")])
      }
      Sheet::Board => ("PCB", &[(Icon::Gear, DOC_SETTINGS), (Icon::Views, NAMED_VIEWS)]),
    };
    let id = egui::Id::new(("ecad-browser", root));
    panels::tree_row(ui, id, 0, Some(true), None, Icon::Component, root, false, theme::TEXT);
    if ui.data(|d| d.get_temp::<bool>(id.with("open"))).unwrap_or(true) {
      for (icon, label) in rows {
        let folder = *label != "Sheet 1/1";
        panels::tree_row(ui, id.with(label), 1, folder.then_some(false), None, *icon, label, false, theme::TEXT);
      }
    }
  });
}

/// The canvas toolbar at the sheet's bottom: info, visibility, zoom in / out / fit, grid,
/// place, stop and select.
pub(crate) fn canvas_toolbar(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, cam: &Cam) {
  let items: [(Glyph, &str); 9] = [
    (Glyph::Info, INFO),
    (Glyph::Std(Icon::Eye), VISIBILITY),
    (Glyph::ZoomIn, ZOOM_IN),
    (Glyph::ZoomOut, ZOOM_OUT),
    (Glyph::Std(Icon::Fit), FIT),
    (Glyph::Std(Icon::Grid), GRID),
    (Glyph::Std(Icon::Plus), PLACE),
    (Glyph::Stop, STOP),
    (Glyph::Std(Icon::Select), SELECT),
  ];
  let size = 26.0;
  let w = items.len() as f32 * size + 12.0;
  let bar = Rect::from_center_size(cam.rect.center_bottom() - vec2(0.0, 24.0), vec2(w, size + 6.0));
  ui.painter().rect(bar, 4.0, theme::PILL, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  let mut hit = None;
  for (i, (g, label)) in items.iter().enumerate() {
    let r = Rect::from_min_size(bar.min + vec2(6.0 + i as f32 * size, 3.0), Vec2::splat(size));
    let resp = ui.interact(r, egui::Id::new(("ecad-canvas-bar", i)), Sense::click());
    let on = *label != VISIBILITY;
    let active = (*label == GRID && v.grid) || (*label == SELECT && v.tool == Tool::Select);
    if active {
      ui.painter().rect_filled(r.shrink(1.0), 2.0, theme::SELECTED);
    } else if resp.hovered() && on {
      ui.painter().rect_filled(r.shrink(1.0), 2.0, theme::HOVER);
    }
    draw_glyph(ui.painter(), r.shrink(5.0), *g, Some(if on { theme::ICON_INK } else { GREYED }));
    let resp = if on { resp.on_hover_text(*label) } else { resp.on_hover_text(unavailable(label)) };
    if resp.clicked() && on {
      hit = Some(*label);
    }
  }
  if let Some(label) = hit {
    edit_cmd(app, v, label);
  }
}

/// What the running command asks for, at the sheet's lower left (Fusion's prompt).
pub(crate) fn prompt_text(v: &EcadView) -> &'static str {
  match (&v.tool, v.sheet) {
    (Tool::Select, _) => "Left-click diagonal drag to select objects",
    (Tool::Place(_), _) => "Left-click to place the part",
    (Tool::Wire, _) => "Left-click to start the net; double-click, right-click or Esc ends it",
    (Tool::Label, _) => "Left-click a net to label it",
    (Tool::NoConnect, _) => "Left-click a pin that stays unconnected",
    (Tool::Delete, Sheet::Schematic) => "Left-click a part, net, label or marker to delete it",
    (Tool::Delete, Sheet::Board) => "Left-click a track, via or hole to delete it",
    (Tool::Route, _) => "Left-click a pad to start routing; L changes layer; double-click ends",
    (Tool::Via, _) => "Left-click to place a via",
  }
}

pub(crate) fn prompt(app: &App, ui: &mut egui::Ui, v: &EcadView, canvas: Rect) {
  if app.status.is_some() {
    // a message from the last command shows there instead
    return;
  }
  let color = if v.sheet == Sheet::Board { Color32::from_gray(210) } else { Color32::from_gray(70) };
  ui.painter().text(
    canvas.left_bottom() + vec2(10.0, -10.0),
    egui::Align2::LEFT_BOTTOM,
    prompt_text(v),
    egui::FontId::proportional(12.0),
    color,
  );
}

// ---------------------------------------------------------------- along the bottom

const BAR_H: f32 = 24.0;
const OPEN_H: f32 = 130.0;

fn text_commands_height(v: &EcadView) -> f32 {
  BAR_H + if v.text_open { OPEN_H } else { 0.0 }
}

fn bottom_height(v: &EcadView) -> f32 {
  BAR_H + if v.text_open || v.sheets_open || v.comments_open { OPEN_H } else { 0.0 }
}

/// A collapsible header along the bottom: an arrow and the title; toggles `open`.
fn bottom_header(ui: &mut egui::Ui, r: Rect, title: &str, open: &mut bool) {
  let resp = ui.interact(r, egui::Id::new(("ecad-bottom", title)), Sense::click());
  let fill = if resp.hovered() { theme::HOVER } else { theme::FLOAT_HEADER };
  ui.painter().rect(r, 0.0, fill, Stroke::new(1.0, Color32::from_gray(212)), egui::StrokeKind::Inside);
  let c = r.left_center() + vec2(12.0, 0.0);
  let pts = if *open {
    vec![c + vec2(-4.0, 2.0), c + vec2(4.0, 2.0), c + vec2(0.0, -3.0)]
  } else {
    vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  };
  ui.painter().add(egui::Shape::convex_polygon(pts, theme::GROUP_LABEL, Stroke::NONE));
  ui.painter().text(
    r.left_center() + vec2(24.0, 0.0),
    egui::Align2::LEFT_CENTER,
    title,
    egui::FontId::proportional(12.0),
    theme::GROUP_LABEL,
  );
  if resp.clicked() {
    *open = !*open;
  }
}

/// TEXT COMMANDS: the header, and while open what was run and the line with Txt / Py / TS.
fn text_commands(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, r: Rect) {
  let header = Rect::from_min_size(pos2(r.min.x, r.max.y - BAR_H), vec2(r.width(), BAR_H));
  let mut open = v.text_open;
  bottom_header(ui, header, "TEXT COMMANDS", &mut open);
  if v.text_open {
    let body = Rect::from_min_max(r.min, pos2(r.max.x, header.min.y));
    text_body(app, ui, v, body);
  }
  v.text_open = open;
}

fn text_body(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, body: Rect) {
  ui.painter().rect(body, 0.0, Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  let mut run = None;
  ui.scope_builder(egui::UiBuilder::new().max_rect(body.shrink(6.0)), |ui| {
    let input_h = 24.0;
    egui::ScrollArea::vertical()
      .id_salt("ecad-text-log")
      .max_height((body.height() - 12.0 - input_h - 6.0).max(10.0))
      .stick_to_bottom(true)
      .auto_shrink([false, false])
      .show(ui, |ui| {
        for l in &v.text_log {
          ui.label(RichText::new(l).monospace().size(11.5).color(theme::TEXT));
        }
      });
    ui.horizontal(|ui| {
      let w = (ui.available_width() - 150.0).max(80.0);
      let resp =
        ui.add(egui::TextEdit::singleline(&mut v.text_input).id(egui::Id::new("ecad-text-input")).desired_width(w));
      if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        run = Some(std::mem::take(&mut v.text_input));
      }
      for (m, label) in [(TextMode::Txt, "Txt"), (TextMode::Py, "Py"), (TextMode::Ts, "TS")] {
        ui.radio_value(&mut v.text_mode, m, label);
      }
    });
  });
  if let Some(line) = run {
    text_command(app, v, &line);
  }
}

/// SHEETS, TEXT COMMANDS and COMMENTS along the editor's bottom; an open one shows above its
/// header.
pub(crate) fn bottom_bar(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView, r: Rect) {
  let side = 220.0_f32.min(r.width() / 4.0);
  let cols = [
    Rect::from_min_max(r.min, pos2(r.min.x + side, r.max.y)),
    Rect::from_min_max(pos2(r.min.x + side, r.min.y), pos2(r.max.x - side, r.max.y)),
    Rect::from_min_max(pos2(r.max.x - side, r.min.y), r.max),
  ];
  ui.painter().rect_filled(r, 0.0, theme::PANEL);
  let header = |c: Rect| Rect::from_min_size(pos2(c.min.x, c.max.y - BAR_H), vec2(c.width(), BAR_H));
  let body = |c: Rect| Rect::from_min_max(c.min, pos2(c.max.x, c.max.y - BAR_H));
  let (mut sheets, mut comments) = (v.sheets_open, v.comments_open);
  bottom_header(ui, header(cols[0]), "SHEETS", &mut sheets);
  if v.sheets_open && body(cols[0]).height() > 10.0 {
    let b = body(cols[0]);
    ui.painter().rect(b, 0.0, Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    let row = Rect::from_min_size(b.min + vec2(6.0, 6.0), vec2(b.width() - 12.0, 22.0));
    ui.painter().rect_filled(row, 2.0, theme::SELECTED);
    ui.painter().text(
      row.left_center() + vec2(8.0, 0.0),
      egui::Align2::LEFT_CENTER,
      "1  Sheet 1",
      egui::FontId::proportional(12.0),
      theme::TEXT,
    );
  }
  let mut open = v.text_open;
  bottom_header(ui, header(cols[1]), "TEXT COMMANDS", &mut open);
  if v.text_open && body(cols[1]).height() > 10.0 {
    text_body(app, ui, v, body(cols[1]));
  }
  bottom_header(ui, header(cols[2]), "COMMENTS", &mut comments);
  if v.comments_open && body(cols[2]).height() > 10.0 {
    let b = body(cols[2]);
    ui.painter().rect(b, 0.0, Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    ui.painter().text(
      b.center(),
      egui::Align2::CENTER_CENTER,
      "No comments",
      egui::FontId::proportional(12.0),
      theme::MUTED,
    );
  }
  v.text_open = open;
  v.sheets_open = sheets;
  v.comments_open = comments;
}
