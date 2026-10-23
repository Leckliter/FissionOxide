//! The electronics documents' canvas: a schematic editor and a board editor over the design's
//! electronics project, EAGLE-style (the ribbon, browser and panels around it are in
//! `ecad_chrome`). Every edit is an `ecad.*` command, so it undoes like any other and scripts
//! can do the same. Wheel zooms, middle- or right-drag pans, F fits, / opens the command line.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use fox_engine::electronics::board::{Layer, Side};
use fox_engine::electronics::geom::{P, Xf, add, rot};
use fox_engine::electronics::library::{FpLayer, Gfx};
use fox_engine::electronics::{Project, Severity, Violation};
use serde_json::json;

use crate::ecad_chrome::{self, Page, TextMode};
use crate::{App, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sheet {
  #[default]
  Schematic,
  Board,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Tool {
  #[default]
  Select,
  /// Placing a device from the library.
  Place(String),
  Wire,
  Label,
  NoConnect,
  Delete,
  Route,
  Via,
}

/// An electronics document's state: which page it shows and its editor.
#[derive(Debug, Clone)]
pub struct EcadView {
  /// The electronics design's start page, or the schematic / board editor.
  pub page: Page,
  /// The ribbon tab shown (an index into the page's tabs).
  pub ribbon: usize,
  pub sheet: Sheet,
  pub tool: Tool,
  /// Zoom (pixels per mm) and pan, per sheet.
  pub cam: [(f32, Vec2); 2],
  pub(crate) fitted: [bool; 2],
  /// Points of the wire or track being drawn (with the layer each run is on).
  pub(crate) pending: Vec<(P, Layer)>,
  pub layer: Layer,
  pub selected: Option<String>,
  pub(crate) drag: Option<(String, P)>,
  pub query: String,
  pub label: String,
  pub pour_net: String,
  /// The last ERC or DRC: title and findings.
  pub report: Option<(String, Vec<Violation>)>,
  /// The command line under the ribbon, and a request to focus it.
  pub cmdline: String,
  pub(crate) focus_cmdline: bool,
  /// TEXT COMMANDS: open, its language, the line being typed and what it printed.
  pub text_open: bool,
  pub text_mode: TextMode,
  pub text_input: String,
  pub text_log: Vec<String>,
  /// SHEETS and COMMENTS along the bottom, open.
  pub sheets_open: bool,
  pub comments_open: bool,
  /// The PLACE COMPONENTS panel: open, and its tab (libraries or recently placed).
  pub place_open: bool,
  pub place_tab: usize,
  /// Devices placed, the latest first.
  pub recent: Vec<String>,
  /// The schematic grid shown.
  pub grid: bool,
  /// The start page's Description, and whether it is being edited.
  pub description: String,
  pub editing_description: bool,
}

impl Default for EcadView {
  fn default() -> Self {
    EcadView {
      page: Page::Editor,
      ribbon: 0,
      sheet: Sheet::Schematic,
      tool: Tool::Select,
      cam: [(4.0, Vec2::ZERO), (12.0, Vec2::ZERO)],
      fitted: [false; 2],
      pending: Vec::new(),
      layer: Layer::Top,
      selected: None,
      drag: None,
      query: String::new(),
      label: "NET1".into(),
      pour_net: "GND".into(),
      report: None,
      cmdline: String::new(),
      focus_cmdline: false,
      text_open: false,
      text_mode: TextMode::Txt,
      text_input: String::new(),
      text_log: Vec::new(),
      sheets_open: false,
      comments_open: false,
      place_open: true,
      place_tab: 0,
      recent: Vec::new(),
      grid: true,
      description: String::new(),
      editing_description: false,
    }
  }
}

const SCH_GRID: f64 = 2.54;
const BOARD_GRID: f64 = 0.25;

fn layer_color(l: Layer, active: bool) -> Color32 {
  let c = match l {
    Layer::Top => Color32::from_rgb(200, 52, 52),
    Layer::Inner1 => Color32::from_rgb(200, 200, 52),
    Layer::Inner2 => Color32::from_rgb(200, 52, 200),
    Layer::Bottom => Color32::from_rgb(60, 100, 210),
  };
  if active { c } else { c.gamma_multiply(0.55) }
}

pub(crate) struct Cam {
  pub(crate) rect: Rect,
  pub(crate) zoom: f32,
  pub(crate) pan: Vec2,
}

impl Cam {
  fn to(&self, p: P) -> Pos2 {
    self.rect.center() + self.pan + vec2(p[0] as f32 * self.zoom, -(p[1] as f32) * self.zoom)
  }

  pub(crate) fn from(&self, s: Pos2) -> P {
    let d = s - self.rect.center() - self.pan;
    [(d.x / self.zoom) as f64, (-d.y / self.zoom) as f64]
  }

  fn len(&self, mm: f64) -> f32 {
    (mm as f32 * self.zoom).max(1.0)
  }
}

/// The BOARD panel's menu: the outline, copper layers and pours.
pub(crate) fn board_menu(app: &mut App, ui: &mut egui::Ui, v: &mut EcadView) {
  {
    ui.set_min_width(220.0);
    // a sketch from the design, or a selected face
    let sketches: Vec<(u32, String)> = app
      .s
      .design
      .features
      .iter()
      .filter(|f| matches!(f.kind, fox_engine::model::FeatureKind::Sketch { .. }))
      .map(|f| (f.id.0, f.name.clone()))
      .collect();
    ui.label(egui::RichText::new("Outline from").color(theme::MUTED));
    for (id, name) in sketches {
      if ui.button(format!("Sketch: {name}")).clicked() {
        app.run("ecad.board.outline", json!({"sketch": id}));
        v.fitted[1] = false;
        ui.close();
      }
    }
    let face = app.s.selection.iter().find_map(|s| match s {
      fox_engine::Sel::Face { body, face } => Some((body.0, *face)),
      _ => None,
    });
    if let Some((b, f)) = face
      && ui.button("The selected face").clicked()
    {
      app.run("ecad.board.outline", json!({"face": {"body": b, "face": f}}));
      v.fitted[1] = false;
      ui.close();
    }
    for (w, h) in [(50.0, 30.0), (80.0, 50.0), (100.0, 80.0)] {
      if ui.button(format!("Rectangle {w} × {h} mm")).clicked() {
        app.run("ecad.board.outline", json!({"rect": [w, h]}));
        v.fitted[1] = false;
        ui.close();
      }
    }
    ui.separator();
    let layers = app.s.design.electronics.as_ref().map_or(2, |p| p.board.layers);
    if ui.button(if layers >= 4 { "Two copper layers" } else { "Four copper layers" }).clicked() {
      app.run("ecad.board.outline", json!({"layers": if layers >= 4 { 2 } else { 4 }}));
      ui.close();
    }
    ui.separator();
    ui.horizontal(|ui| {
      ui.label("Pour net");
      ui.add(egui::TextEdit::singleline(&mut v.pour_net).desired_width(60.0));
    });
    for l in [Layer::Top, Layer::Bottom] {
      if ui.button(format!("Pour {} on {}", v.pour_net, l.name())).clicked() {
        app.run("ecad.pour", json!({"net": v.pour_net, "layer": l.name()}));
        ui.close();
      }
    }
  }
}

/// An electronics document's page: the start page, or the editor as Fusion's: the command row
/// under the ribbon, PLACE COMPONENTS at the right, the sheet with the browser, the canvas
/// toolbar and the prompt over it, and SHEETS, TEXT COMMANDS and COMMENTS along the bottom.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
  let Some(mut v) = app.electronics.take() else { return };
  if v.page == Page::Start {
    ecad_chrome::start_page(app, ui, &mut v);
    app.electronics = Some(v);
    return;
  }
  let full = ui.available_rect_before_wrap();
  let l = ecad_chrome::EditorLayout::new(full, &v);
  let rect = l.canvas;
  let resp = ui.allocate_rect(rect, Sense::click_and_drag());
  app.canvas = rect;
  let k = if v.sheet == Sheet::Schematic { 0 } else { 1 };
  if !v.fitted[k] {
    if let Some(pr) = app.s.design.electronics.as_deref() {
      v.cam[k] = fit(pr, v.sheet, rect);
    }
    v.fitted[k] = true;
  }
  let (mut zoom, mut pan) = v.cam[k];
  if resp.hovered() {
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll != 0.0 {
      let f = (scroll * 0.002).exp();
      let new = (zoom * f).clamp(0.3, 400.0);
      if let Some(m) = resp.hover_pos() {
        let c = rect.center() + pan;
        pan += (m - c) * (1.0 - new / zoom);
      }
      zoom = new;
    }
  }
  if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
    pan += resp.drag_delta();
  }
  v.cam[k] = (zoom, pan);
  let cam = Cam { rect, zoom, pan };
  let painter = ui.painter_at(rect);
  // Fusion's schematic is a white sheet; the board stays dark
  let bg = if v.sheet == Sheet::Schematic { Color32::WHITE } else { Color32::from_rgb(28, 28, 30) };
  painter.rect_filled(rect, 0.0, bg);
  if v.grid {
    grid(&painter, &cam, v.sheet);
  }
  let hover = resp.hover_pos().map(|h| cam.from(h));
  if let Some(pr) = app.s.design.electronics.as_deref() {
    match v.sheet {
      Sheet::Schematic => draw_schematic(&painter, &cam, pr, &v),
      Sheet::Board => draw_board(&painter, &cam, pr, &v),
    }
  }
  let snapped = hover.map(|h| snap_point(app.s.design.electronics.as_deref(), &v, h));
  // the ghost of what is being drawn
  if let Some(snapped) = snapped {
    if !v.pending.is_empty() {
      let mut pts: Vec<Pos2> = v.pending.iter().map(|q| cam.to(q.0)).collect();
      let last = v.pending.last().map_or(snapped, |q| q.0);
      let corner = if v.sheet == Sheet::Schematic { [snapped[0], last[1]] } else { snapped };
      pts.push(cam.to(corner));
      pts.push(cam.to(snapped));
      let color = if v.sheet == Sheet::Schematic { Color32::from_rgb(30, 130, 30) } else { layer_color(v.layer, true) };
      let w = if v.sheet == Sheet::Schematic {
        2.0
      } else {
        cam.len(app.s.design.electronics.as_ref().map_or(0.25, |p| p.board.rules.track_width))
      };
      painter.add(egui::Shape::line(pts, Stroke::new(w, color.gamma_multiply(0.7))));
    }
    if matches!(v.tool, Tool::Wire | Tool::Route | Tool::Via | Tool::Label | Tool::NoConnect | Tool::Place(_)) {
      let c = cam.to(snapped);
      let col = if v.sheet == Sheet::Schematic { Color32::from_gray(90) } else { Color32::from_gray(220) };
      painter.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], Stroke::new(1.0, col));
      painter.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], Stroke::new(1.0, col));
    }
    if let Tool::Place(d) = &v.tool {
      painter.text(
        cam.to(snapped) + vec2(10.0, -10.0),
        egui::Align2::LEFT_BOTTOM,
        d,
        egui::FontId::proportional(12.0),
        theme::ACCENT,
      );
    }
  }
  ecad_chrome::command_row(app, ui, &mut v, l.command, snapped);
  ecad_chrome::place_panel(app, ui, &mut v, l.place);
  ecad_chrome::editor_browser(app, ui, &mut v, rect);
  ecad_chrome::canvas_toolbar(app, ui, &mut v, &cam);
  ecad_chrome::prompt(app, ui, &v, rect);
  ecad_chrome::bottom_bar(app, ui, &mut v, l.bottom);
  input(app, ui, &resp, &cam, &mut v, hover);
  report_window(ui.ctx(), &mut v, &cam);
  app.electronics = Some(v);
}

fn grid(p: &egui::Painter, cam: &Cam, sheet: Sheet) {
  let step = if sheet == Sheet::Schematic { SCH_GRID * 4.0 } else { 1.0 };
  let px = step as f32 * cam.zoom;
  if px < 12.0 {
    return;
  }
  let lo = cam.from(cam.rect.left_bottom());
  let hi = cam.from(cam.rect.right_top());
  let color = if sheet == Sheet::Schematic { Color32::from_gray(232) } else { Color32::from_gray(48) };
  let mut x = (lo[0] / step).floor() * step;
  while x <= hi[0] {
    let a = cam.to([x, lo[1]]);
    p.line_segment([pos2(a.x, cam.rect.top()), pos2(a.x, cam.rect.bottom())], Stroke::new(1.0, color));
    x += step;
  }
  let mut y = (lo[1] / step).floor() * step;
  while y <= hi[1] {
    let a = cam.to([lo[0], y]);
    p.line_segment([pos2(cam.rect.left(), a.y), pos2(cam.rect.right(), a.y)], Stroke::new(1.0, color));
    y += step;
  }
}

/// A view that shows everything on the sheet.
fn fit(pr: &Project, sheet: Sheet, rect: Rect) -> (f32, Vec2) {
  let libs = pr.libs();
  let pts: Vec<P> = match sheet {
    Sheet::Schematic => {
      let mut v: Vec<P> = pr.schematic.wires.iter().flat_map(|w| [w.a, w.b]).collect();
      for part in &pr.schematic.parts {
        v.push(add(part.at, [-8.0, -8.0]));
        v.push(add(part.at, [8.0, 8.0]));
        let _ = &libs;
      }
      v
    }
    Sheet::Board => {
      let mut v = pr.board.outline.clone();
      v.extend(pr.board.placements.iter().map(|p| p.at));
      v
    }
  };
  let Some((lo, hi)) = fox_engine::electronics::geom::bounds(pts) else {
    return (if sheet == Sheet::Board { 12.0 } else { 4.0 }, Vec2::ZERO);
  };
  let (w, h) = ((hi[0] - lo[0]).max(10.0) as f32, (hi[1] - lo[1]).max(10.0) as f32);
  let zoom = ((rect.width() - 60.0) / w).min((rect.height() - 60.0) / h).clamp(0.3, 400.0);
  let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
  (zoom, vec2(-(c[0] as f32) * zoom, c[1] as f32 * zoom))
}

/// The cursor snapped to the grid, or to a pin (schematic) or pad (board) under it.
fn snap_point(pr: Option<&Project>, v: &EcadView, h: P) -> P {
  let Some(pr) = pr else { return h };
  match v.sheet {
    Sheet::Schematic => {
      let pins = fox_engine::electronics::schematic::placed_pins(&pr.schematic, &pr.libs());
      if let Some(p) = pins.iter().filter(|p| !p.hidden).min_by(|a, b| dist(a.at, h).total_cmp(&dist(b.at, h)))
        && dist(p.at, h) < 1.3
      {
        return p.at;
      }
      fox_engine::electronics::geom::snap(h, SCH_GRID)
    }
    Sheet::Board => {
      let pads = pr.pads();
      if let Some(p) = pads.iter().find(|p| p.shape2(0.0).contains(h)) {
        return p.at;
      }
      if let Some(t) = pr.board.tracks.iter().flat_map(|t| [t.a, t.b]).find(|q| dist(*q, h) < 0.3) {
        return t;
      }
      fox_engine::electronics::geom::snap(h, BOARD_GRID)
    }
  }
}

fn dist(a: P, b: P) -> f64 {
  fox_engine::electronics::geom::dist(a, b)
}

/// The schematic part or board footprint under a point (the smallest that holds it).
fn part_at(pr: &Project, sheet: Sheet, h: P) -> Option<String> {
  let libs = pr.libs();
  let mut best: Option<(f64, String)> = None;
  match sheet {
    Sheet::Schematic => {
      for part in &pr.schematic.parts {
        let Some(sym) = libs.device(&part.device).and_then(|d| libs.symbol(&d.symbol)) else { continue };
        let xf = part.xf();
        let pts = sym
          .gfx
          .iter()
          .flat_map(|g| g.polylines().into_iter().flatten())
          .chain(sym.pins.iter().map(|p| p.at))
          .map(|q| xf.apply(q));
        if let Some((lo, hi)) = fox_engine::electronics::geom::bounds(pts) {
          let (lo, hi) = (add(lo, [-0.5, -0.5]), add(hi, [0.5, 0.5]));
          if h[0] >= lo[0] && h[0] <= hi[0] && h[1] >= lo[1] && h[1] <= hi[1] {
            let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
            if best.as_ref().is_none_or(|b| area < b.0) {
              best = Some((area, part.refdes.clone()));
            }
          }
        }
      }
    }
    Sheet::Board => {
      for pl in &pr.board.placements {
        let Some(part) = pr.schematic.parts.iter().find(|p| p.refdes == pl.refdes) else { continue };
        let Some(fp) = libs
          .device(&part.device)
          .and_then(|d| libs.footprint(if pl.footprint.is_empty() { &d.footprint } else { &pl.footprint }))
        else {
          continue;
        };
        let (lo, hi) = fp.courtyard();
        let xf = pl.xf();
        let poly: Vec<P> = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]].iter().map(|q| xf.apply(*q)).collect();
        if fox_engine::electronics::geom::point_in_poly(h, &poly) {
          let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
          if best.as_ref().is_none_or(|b| area < b.0) {
            best = Some((area, pl.refdes.clone()));
          }
        }
      }
    }
  }
  best.map(|b| b.1)
}

fn input(app: &mut App, ui: &egui::Ui, resp: &egui::Response, cam: &Cam, v: &mut EcadView, hover: Option<P>) {
  let ctx = ui.ctx().clone();
  let typing = ctx.egui_wants_keyboard_input();
  let key = |k: egui::Key| !typing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
  if !typing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
    app.s.undo();
  }
  if !typing && (ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y))) {
    app.s.redo();
  }
  if key(egui::Key::Escape) {
    if !v.pending.is_empty() {
      finish(app, v);
    } else {
      v.tool = Tool::Select;
      v.selected = None;
    }
  }
  if key(egui::Key::F) && v.selected.is_none() {
    v.fitted = [false; 2];
  }
  // Fusion: / activates the command line
  if key(egui::Key::Slash) {
    ctx.memory_mut(|m| m.request_focus(ecad_chrome::cmdline_id()));
  }
  let Some(h) = hover else { return };
  // a copy: the commands below change the design
  let pr_owned = app.s.design.electronics.clone();
  let pr_ref = pr_owned.as_deref();
  let snapped = snap_point(pr_ref, v, h);
  // keys on the selection
  if let Some(sel) = v.selected.clone() {
    if key(egui::Key::R) {
      match v.sheet {
        Sheet::Schematic => {
          let r = pr_ref.and_then(|p| p.schematic.parts.iter().find(|x| x.refdes == sel)).map_or(0.0, |x| x.rot);
          app.run("ecad.part.edit", json!({"refdes": sel, "rot": r + 90.0}));
        }
        Sheet::Board => {
          let r = pr_ref.and_then(|p| p.board.placement(&sel)).map_or(0.0, |x| x.rot);
          app.run("ecad.place", json!({"refdes": sel, "rot": r + 90.0}));
        }
      }
    } else if v.sheet == Sheet::Schematic && key(egui::Key::M) {
      let m = pr_ref.and_then(|p| p.schematic.parts.iter().find(|x| x.refdes == sel)).is_some_and(|x| x.mirror);
      app.run("ecad.part.edit", json!({"refdes": sel, "mirror": !m}));
    } else if v.sheet == Sheet::Board && key(egui::Key::F) {
      let bottom = pr_ref.and_then(|p| p.board.placement(&sel)).is_some_and(|x| x.side == Side::Bottom);
      app.run("ecad.place", json!({"refdes": sel, "side": if bottom { "top" } else { "bottom" }}));
    } else if v.sheet == Sheet::Schematic && (key(egui::Key::Delete) || key(egui::Key::Backspace)) {
      app.run("ecad.part.delete", json!({"refdes": sel}));
      v.selected = None;
    }
  }
  if v.tool == Tool::Route && key(egui::Key::L) {
    // change layer through a via at the last point
    let layers = app.s.design.electronics.as_ref().map(|p| p.board.copper_layers()).unwrap_or_default();
    if let Some(i) = layers.iter().position(|l| *l == v.layer) {
      let next = layers[(i + 1) % layers.len().max(1)];
      if let Some(&(last, _)) = v.pending.last() {
        let net = commit_run(app, v);
        app.run(
          "ecad.via",
          match &net {
            Some(n) => json!({"at": last, "net": n}),
            None => json!({"at": last}),
          },
        );
        v.pending = vec![(last, next)];
      }
      v.layer = next;
    }
  }
  if resp.double_clicked() && !v.pending.is_empty() {
    finish(app, v);
    return;
  }
  if resp.secondary_clicked() && !v.pending.is_empty() {
    finish(app, v);
    return;
  }
  // dragging parts with Select
  if v.tool == Tool::Select {
    // where the button went down, not where the drag was recognised
    let origin = ctx.input(|i| i.pointer.press_origin()).map_or(h, |o| cam.from(o));
    if resp.drag_started_by(egui::PointerButton::Primary)
      && let Some(pr) = pr_ref
      && let Some(r) = part_at(pr, v.sheet, origin)
    {
      let at = match v.sheet {
        Sheet::Schematic => pr.schematic.parts.iter().find(|p| p.refdes == r).map(|p| p.at),
        Sheet::Board => pr.board.placement(&r).map(|p| p.at),
      };
      if let Some(at) = at {
        v.drag = Some((r.clone(), fox_engine::electronics::geom::sub(at, origin)));
        v.selected = Some(r);
      }
    }
    if resp.drag_stopped()
      && let Some((r, off)) = v.drag.take()
    {
      let grid = if v.sheet == Sheet::Schematic { SCH_GRID } else { BOARD_GRID };
      let at = fox_engine::electronics::geom::snap(add(h, off), grid);
      match v.sheet {
        Sheet::Schematic => app.run("ecad.part.edit", json!({"refdes": r, "at": at})),
        Sheet::Board => app.run("ecad.place", json!({"refdes": r, "at": at})),
      };
    }
    if let Some((r, off)) = &v.drag {
      let grid = if v.sheet == Sheet::Schematic { SCH_GRID } else { BOARD_GRID };
      let at = fox_engine::electronics::geom::snap(add(h, *off), grid);
      ui.painter_at(cam.rect).text(
        cam.to(at),
        egui::Align2::CENTER_CENTER,
        format!("{r} ✚"),
        egui::FontId::proportional(13.0),
        theme::ACCENT,
      );
    }
  }
  if !resp.clicked() {
    return;
  }
  match v.tool.clone() {
    Tool::Select => v.selected = pr_ref.and_then(|pr| part_at(pr, v.sheet, h)),
    Tool::Place(device) => {
      let at = fox_engine::electronics::geom::snap(h, SCH_GRID);
      if let Some(r) = app.run("ecad.part.add", json!({"device": device, "at": at})) {
        app.status = Some((format!("Placed {}", r["refdes"].as_str().unwrap_or("")), false));
        v.recent.retain(|d| *d != device);
        v.recent.insert(0, device);
      }
    }
    Tool::Wire => {
      if let Some(&(last, _)) = v.pending.last() {
        // orthogonal: an elbow at the corner
        let corner = [snapped[0], last[1]];
        if dist(corner, last) > 1e-6 && dist(corner, snapped) > 1e-6 {
          v.pending.push((corner, v.layer));
        }
        v.pending.push((snapped, v.layer));
        // ending on a pin or another wire finishes the wire
        let ends_on_pin = pr_ref.is_some_and(|pr| {
          fox_engine::electronics::schematic::placed_pins(&pr.schematic, &pr.libs())
            .iter()
            .any(|p| fox_engine::electronics::geom::near(p.at, snapped))
            || pr
              .schematic
              .wires
              .iter()
              .any(|w| fox_engine::electronics::geom::point_seg_dist(snapped, w.a, w.b) < 1e-4)
        });
        if ends_on_pin && v.pending.len() > 1 {
          finish(app, v);
        }
      } else {
        v.pending.push((snapped, v.layer));
      }
    }
    Tool::Label => {
      app.run("ecad.label", json!({"name": v.label, "at": snapped}));
    }
    Tool::NoConnect => {
      app.run("ecad.no_connect", json!({"at": snapped}));
    }
    Tool::Delete => match v.sheet {
      Sheet::Schematic => {
        let r = app.run("ecad.schematic.delete", json!({"at": h, "tolerance": (6.0 / cam.zoom) as f64}));
        if r.is_some_and(|r| r["deleted"] == 0)
          && let Some(part) = pr_ref.and_then(|pr| part_at(pr, v.sheet, h))
        {
          app.run("ecad.part.delete", json!({"refdes": part}));
        }
      }
      Sheet::Board => {
        app.run("ecad.board.delete", json!({"at": h}));
      }
    },
    Tool::Route => {
      v.pending.push((snapped, v.layer));
    }
    Tool::Via => {
      app.run("ecad.via", json!({"at": snapped}));
    }
  }
}

/// Sends the track run being drawn; returns its net.
fn commit_run(app: &mut App, v: &mut EcadView) -> Option<String> {
  let pts: Vec<P> = v.pending.iter().map(|q| q.0).collect();
  if pts.len() < 2 {
    return None;
  }
  let layer = v.pending.last().map_or(v.layer, |q| q.1);
  app
    .run("ecad.route", json!({"points": pts, "layer": layer.name()}))
    .and_then(|r| r["net"].as_str().map(str::to_string))
}

pub(crate) fn finish(app: &mut App, v: &mut EcadView) {
  match v.sheet {
    Sheet::Schematic => {
      let mut pts: Vec<P> = v.pending.iter().map(|q| q.0).collect();
      pts.dedup_by(|a, b| fox_engine::electronics::geom::near(*a, *b));
      if pts.len() >= 2 {
        app.run("ecad.wire", json!({"points": pts, "direct": true}));
      }
    }
    Sheet::Board => {
      commit_run(app, v);
    }
  }
  v.pending.clear();
}

fn report_window(ctx: &egui::Context, v: &mut EcadView, cam: &Cam) {
  let Some((title, items)) = v.report.clone() else { return };
  let mut open = true;
  let mut goto = None;
  egui::Window::new(&title)
    .open(&mut open)
    .default_pos(cam.rect.right_top() + vec2(-360.0, 20.0))
    .default_width(340.0)
    .show(ctx, |ui| {
      if items.is_empty() {
        ui.label(egui::RichText::new("No problems found").color(Color32::from_rgb(30, 140, 60)));
      }
      egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for it in &items {
          let color = if it.severity == Severity::Error { theme::ERROR } else { Color32::from_rgb(190, 130, 20) };
          let r = ui
            .add(egui::Label::new(egui::RichText::new(format!("• {}", it.message)).color(color)).sense(Sense::click()));
          if r.clicked() {
            goto = it.at;
          }
        }
      });
    });
  if !open {
    v.report = None;
  }
  if let Some(at) = goto {
    let k = if v.sheet == Sheet::Schematic { 0 } else { 1 };
    let zoom = v.cam[k].0;
    v.cam[k].1 = vec2(-(at[0] as f32) * zoom, at[1] as f32 * zoom);
  }
}

fn gfx(p: &egui::Painter, cam: &Cam, g: &Gfx, xf: &Xf, color: Color32, fill: Color32) {
  match g {
    Gfx::Text { at, text, size } => {
      p.text(
        cam.to(xf.apply(*at)),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(cam.len(*size)),
        color,
      );
    }
    Gfx::Circle { c, r, width, fill: f } => {
      let c = cam.to(xf.apply(*c));
      if *f {
        p.circle_filled(c, cam.len(*r), color);
      } else {
        p.circle_stroke(c, cam.len(*r), Stroke::new(cam.len(width.max(0.15)), color));
      }
    }
    _ => {
      let filled = matches!(g, Gfx::Rect { fill: true, .. } | Gfx::Poly { fill: true, .. });
      for line in g.polylines() {
        let pts: Vec<Pos2> = line.iter().map(|q| cam.to(xf.apply(*q))).collect();
        if filled && pts.len() >= 3 {
          p.add(egui::Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
        }
        p.add(egui::Shape::line(pts, Stroke::new(cam.len(g.width().max(0.15)), color)));
      }
    }
  }
}

fn draw_schematic(p: &egui::Painter, cam: &Cam, pr: &Project, v: &EcadView) {
  let libs = pr.libs();
  let ink = Color32::from_rgb(139, 26, 26);
  let wire = Color32::from_rgb(26, 122, 26);
  let font = |mm: f64| egui::FontId::proportional(cam.len(mm));
  for part in &pr.schematic.parts {
    let Some(dev) = libs.device(&part.device) else { continue };
    let Some(sym) = libs.symbol(&dev.symbol) else { continue };
    let xf = part.xf();
    let sel = v.selected.as_deref() == Some(part.refdes.as_str());
    let color = if sel { theme::ACCENT } else { ink };
    for g in &sym.gfx {
      gfx(p, cam, g, &xf, color, Color32::from_rgb(245, 230, 200));
    }
    for pin in sym.pins.iter().filter(|q| !q.hidden) {
      let a = xf.apply(pin.at);
      let b = xf.apply(add(pin.at, rot([pin.length, 0.0], pin.dir)));
      p.line_segment([cam.to(a), cam.to(b)], Stroke::new(cam.len(0.2), color));
      if !pin.name.is_empty() && !sym.power && sym.pins.len() > 2 && cam.zoom > 2.5 {
        let inward = xf.angle(pin.dir).to_radians();
        let at = add(b, rot([0.6, 0.0], xf.angle(pin.dir)));
        let align = if inward.cos() < -0.5 {
          egui::Align2::RIGHT_CENTER
        } else if inward.cos() > 0.5 {
          egui::Align2::LEFT_CENTER
        } else {
          egui::Align2::CENTER_CENTER
        };
        p.text(cam.to(at), align, &pin.name, font(1.1), Color32::from_gray(60));
      }
    }
    let pts = sym.gfx.iter().flat_map(|g| g.polylines().into_iter().flatten()).map(|q| xf.apply(q));
    let (lo, hi) = fox_engine::electronics::geom::bounds(pts).unwrap_or((part.at, part.at));
    if sym.power {
      let ground = sym.gfx.iter().flat_map(|g| g.polylines().into_iter().flatten()).any(|q| q[1] < -0.5);
      let at = xf.apply(if ground { [0.0, -3.4] } else { [0.0, 2.2] });
      p.text(cam.to(at), egui::Align2::CENTER_CENTER, &part.value, font(1.3), Color32::from_gray(50));
    } else {
      let mid = (lo[1] + hi[1]) / 2.0;
      p.text(
        cam.to([hi[0] + 0.8, mid + 0.9]),
        egui::Align2::LEFT_CENTER,
        &part.refdes,
        font(1.5),
        Color32::from_gray(40),
      );
      p.text(
        cam.to([hi[0] + 0.8, mid - 0.9]),
        egui::Align2::LEFT_CENTER,
        &part.value,
        font(1.3),
        Color32::from_gray(90),
      );
    }
  }
  for w in &pr.schematic.wires {
    p.line_segment([cam.to(w.a), cam.to(w.b)], Stroke::new(cam.len(0.25), wire));
  }
  for w in &pr.schematic.wires {
    for e in [w.a, w.b] {
      let n =
        pr.schematic.wires.iter().filter(|o| fox_engine::electronics::geom::point_seg_dist(e, o.a, o.b) < 1e-4).count();
      let tee = pr.schematic.wires.iter().any(|o| {
        fox_engine::electronics::geom::point_seg_dist(e, o.a, o.b) < 1e-4
          && !fox_engine::electronics::geom::near(e, o.a)
          && !fox_engine::electronics::geom::near(e, o.b)
      });
      if n >= 3 || tee {
        p.circle_filled(cam.to(e), cam.len(0.5), wire);
      }
    }
  }
  for l in &pr.schematic.labels {
    let from = pr.schematic.wires.iter().find_map(|w| {
      if fox_engine::electronics::geom::near(w.a, l.at) {
        Some(w.b)
      } else if fox_engine::electronics::geom::near(w.b, l.at) {
        Some(w.a)
      } else {
        None
      }
    });
    let left = from.is_some_and(|f| f[0] > l.at[0] + 1e-6);
    let (dx, align) = if left { (-0.3, egui::Align2::RIGHT_BOTTOM) } else { (0.3, egui::Align2::LEFT_BOTTOM) };
    p.text(cam.to([l.at[0] + dx, l.at[1] + 0.2]), align, &l.name, font(1.4), Color32::from_rgb(26, 74, 139));
  }
  for b in &pr.schematic.buses {
    let pts: Vec<Pos2> = b.pts.iter().map(|q| cam.to(*q)).collect();
    p.add(egui::Shape::line(pts, Stroke::new(cam.len(0.6), Color32::from_rgb(26, 58, 155))));
    if let Some(q) = b.pts.first() {
      p.text(cam.to([q[0], q[1] + 0.6]), egui::Align2::LEFT_BOTTOM, &b.name, font(1.4), Color32::from_rgb(26, 58, 155));
    }
  }
  for nc in &pr.schematic.no_connects {
    let d = 0.7;
    let c = Color32::from_rgb(26, 26, 155);
    p.line_segment([cam.to([nc[0] - d, nc[1] - d]), cam.to([nc[0] + d, nc[1] + d])], Stroke::new(cam.len(0.2), c));
    p.line_segment([cam.to([nc[0] - d, nc[1] + d]), cam.to([nc[0] + d, nc[1] - d])], Stroke::new(cam.len(0.2), c));
  }
  if let Some((_, items)) = &v.report {
    marks(p, cam, items);
  }
}

fn marks(p: &egui::Painter, cam: &Cam, items: &[Violation]) {
  for it in items {
    if let Some(at) = it.at {
      let c = if it.severity == Severity::Error { theme::ERROR } else { Color32::from_rgb(220, 150, 20) };
      p.circle_stroke(cam.to(at), 9.0, Stroke::new(2.0, c));
    }
  }
}

fn draw_board(p: &egui::Painter, cam: &Cam, pr: &Project, v: &EcadView) {
  let b = &pr.board;
  let libs = pr.libs();
  if b.outline.len() >= 3 {
    let pts: Vec<Pos2> = b.outline.iter().map(|q| cam.to(*q)).collect();
    // concave outlines: the fill is drawn as a fan only when convex; the edge always
    p.add(egui::Shape::closed_line(pts, Stroke::new(2.0, Color32::from_rgb(220, 220, 160))));
  }
  let mut layers = b.copper_layers();
  layers.reverse();
  // the routing layer on top
  layers.retain(|l| *l != v.layer);
  layers.push(v.layer);
  for l in &layers {
    let active = *l == v.layer;
    for pour in b.pours.iter().filter(|q| q.layer == *l && q.outline.len() >= 3) {
      let pts: Vec<Pos2> = pour.outline.iter().map(|q| cam.to(*q)).collect();
      p.add(egui::Shape::closed_line(pts, Stroke::new(1.5, layer_color(*l, active).gamma_multiply(0.6))));
      p.text(
        cam.to(pour.outline[0]) + vec2(4.0, 4.0),
        egui::Align2::LEFT_TOP,
        format!("{} pour", pour.net),
        egui::FontId::proportional(11.0),
        layer_color(*l, active),
      );
    }
    for t in b.tracks.iter().filter(|t| t.layer == *l) {
      p.line_segment([cam.to(t.a), cam.to(t.b)], Stroke::new(cam.len(t.width), layer_color(*l, active)));
      p.circle_filled(cam.to(t.a), cam.len(t.width / 2.0), layer_color(*l, active));
      p.circle_filled(cam.to(t.b), cam.len(t.width / 2.0), layer_color(*l, active));
    }
  }
  let pads = pr.pads();
  for pad in &pads {
    let color = if pad.drill.is_some() {
      Color32::from_rgb(200, 168, 50)
    } else if pad.side == Side::Top {
      layer_color(Layer::Top, v.layer == Layer::Top)
    } else {
      layer_color(Layer::Bottom, v.layer == Layer::Bottom)
    };
    match pad.shape2(0.0) {
      fox_engine::electronics::geom::Shape2::Poly(pts) => {
        p.add(egui::Shape::convex_polygon(pts.iter().map(|q| cam.to(*q)).collect(), color, Stroke::NONE));
      }
      fox_engine::electronics::geom::Shape2::Capsule { a, b: e, r } => {
        p.line_segment([cam.to(a), cam.to(e)], Stroke::new(cam.len(2.0 * r), color));
        p.circle_filled(cam.to(a), cam.len(r), color);
        p.circle_filled(cam.to(e), cam.len(r), color);
      }
    }
    if let Some(d) = pad.drill {
      p.circle_filled(cam.to(pad.at), cam.len(d / 2.0), Color32::from_gray(15));
    }
  }
  for via in &b.vias {
    p.circle_filled(cam.to(via.at), cam.len(via.diameter / 2.0), Color32::from_rgb(200, 168, 50));
    p.circle_filled(cam.to(via.at), cam.len(via.drill / 2.0), Color32::from_gray(15));
  }
  for h in &b.holes {
    p.circle_stroke(cam.to(h.at), cam.len(h.drill / 2.0), Stroke::new(1.5, Color32::from_gray(160)));
  }
  for side in [Side::Bottom, Side::Top] {
    let c = if side == Side::Top { Color32::from_gray(235) } else { Color32::from_rgb(150, 150, 210) };
    for (w, pts) in fox_engine::electronics::gerber::silk_strokes(pr, side) {
      let s: Vec<Pos2> = pts.iter().map(|q| cam.to(*q)).collect();
      p.add(egui::Shape::line(s, Stroke::new(cam.len(w), c)));
    }
  }
  if let Some(sel) = &v.selected
    && let Some(pl) = b.placement(sel)
    && let Some(part) = pr.schematic.parts.iter().find(|q| &q.refdes == sel)
    && let Some(fp) = libs
      .device(&part.device)
      .and_then(|d| libs.footprint(if pl.footprint.is_empty() { &d.footprint } else { &pl.footprint }))
  {
    let xf = pl.xf();
    for g in fp.gfx.iter().filter(|g| g.layer == FpLayer::Courtyard) {
      gfx(p, cam, &g.gfx, &xf, theme::ACCENT, Color32::TRANSPARENT);
    }
  }
  for a in b.ratsnest(&pads) {
    p.line_segment([cam.to(a.a), cam.to(a.b)], Stroke::new(1.0, Color32::from_gray(230)));
  }
  if let Some((_, items)) = &v.report {
    marks(p, cam, items);
  }
}

#[cfg(test)]
mod tests {
  use egui::{Event, Pos2, RawInput, Rect, vec2};
  use fox_engine::Session;
  use serde_json::json;

  use super::{Sheet, Tool};
  use crate::App;

  struct H {
    ctx: egui::Context,
    app: App,
    time: f64,
  }

  impl H {
    fn new() -> H {
      let ctx = egui::Context::default();
      crate::theme::apply(&ctx);
      let mut h = H { ctx, app: App::with_session(Session::default()), time: 0.0 };
      h.frames(2, vec![]);
      h
    }

    fn frames(&mut self, n: usize, events: Vec<Event>) {
      let mut events = Some(events);
      for _ in 0..n {
        self.time += 1.0 / 60.0;
        let input = RawInput {
          screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
          time: Some(self.time),
          events: events.take().unwrap_or_default(),
          ..Default::default()
        };
        let app = &mut self.app;
        self.ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
      }
    }

    fn click(&mut self, pos: Pos2) {
      let b = |pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
      };
      self.frames(1, vec![Event::PointerMoved(pos)]);
      self.frames(1, vec![b(true)]);
      self.frames(2, vec![b(false)]);
    }

    fn set_tool(&mut self, t: Tool) {
      if let Some(v) = self.app.electronics.as_mut() {
        v.tool = t;
      }
    }
  }

  #[test]
  fn workspace_places_parts_and_draws_wires() {
    let mut h = H::new();
    crate::ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
    h.frames(2, vec![]);
    assert!(h.app.electronics.is_some());
    let c = h.app.canvas.center();
    h.set_tool(Tool::Place("R_0603".into()));
    h.click(c);
    h.click(c + vec2(120.0, 0.0));
    let pr = h.app.s.design.electronics.as_deref().expect("placing a part starts the project");
    assert_eq!(pr.schematic.parts.len(), 2);
    assert_eq!(pr.board.placements.len(), 2);
    // a wire between the two resistors' top pins
    let (a, b) = {
      let pins = fox_engine::electronics::schematic::placed_pins(&pr.schematic, &pr.libs());
      (
        pins.iter().find(|p| p.refdes == "R1" && p.number == "1").unwrap().at,
        pins.iter().find(|p| p.refdes == "R2" && p.number == "1").unwrap().at,
      )
    };
    let (zoom, pan) = h.app.electronics.as_ref().unwrap().cam[0];
    let rect = h.app.canvas;
    let to = |p: [f64; 2]| rect.center() + pan + vec2(p[0] as f32 * zoom, -(p[1] as f32) * zoom);
    h.set_tool(Tool::Wire);
    h.click(to(a));
    h.click(to(b));
    let pr = h.app.s.design.electronics.as_deref().unwrap();
    assert_eq!(pr.netlist().net_of("R1", "1"), pr.netlist().net_of("R2", "1"));
    // the design's tab is in the Design workspace
    crate::documents::switch_to(&mut h.app, 0);
    h.frames(2, vec![]);
    assert!(h.app.electronics.is_none());
  }

  #[test]
  fn example_board_draws_and_parts_drag() {
    let mut h = H::new();
    crate::ecad_chrome::new_editor(&mut h.app, Sheet::Schematic);
    h.app.run("ecad.new", json!({"example": true}));
    h.frames(3, vec![]);
    if let Some(v) = h.app.electronics.as_mut() {
      v.sheet = Sheet::Board;
    }
    h.frames(3, vec![]);
    let (zoom, pan) = h.app.electronics.as_ref().unwrap().cam[1];
    let rect = h.app.canvas;
    let to = |p: [f64; 2]| rect.center() + pan + vec2(p[0] as f32 * zoom, -(p[1] as f32) * zoom);
    let before = h.app.s.design.electronics.as_ref().unwrap().board.placement("J2").unwrap().at;
    let a = to(before);
    let b = a + vec2(0.0, 2.0 * zoom);
    let btn = |pos, pressed| Event::PointerButton {
      pos,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    h.frames(1, vec![Event::PointerMoved(a)]);
    h.frames(1, vec![btn(a, true)]);
    for i in 1..=6 {
      h.frames(1, vec![Event::PointerMoved(a + (b - a) * i as f32 / 6.0)]);
    }
    h.frames(2, vec![btn(b, false)]);
    let after = h.app.s.design.electronics.as_ref().unwrap().board.placement("J2").unwrap().at;
    assert!(
      (after[1] - (before[1] - 2.0)).abs() < 1e-6 && (after[0] - before[0]).abs() < 1e-6,
      "{before:?} → {after:?}"
    );
    h.app.s.undo();
    assert_eq!(h.app.s.design.electronics.as_ref().unwrap().board.placement("J2").unwrap().at, before);
  }
}
