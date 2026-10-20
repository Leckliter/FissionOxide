//! The Drawing workspace's canvas: the open drawing's sheet, drawn from the same vector plot the
//! PDF / DXF / SVG exports are written from. Wheel zooms, right- or middle-drag pans,
//! double-click fits the sheet. Tabs along the bottom switch sheets; the + tab adds one.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use fox_engine::model::plot::{Anchor, Item, Pen};
use serde_json::json;

use crate::forms::{Form, Input};
use crate::icons::Icon;
use crate::{App, theme};

const BACKDROP: Color32 = Color32::from_rgb(214, 218, 224);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  let Some(id) = app.drawing else { return };
  let Some(d) = app.s.design.drawing(id) else {
    app.drawing = None;
    return;
  };
  let sheets = d.sheet_list();
  let on = match app.s.drawing_sheet {
    Some((dr, sh)) if dr == id && d.has_sheet(sh) => sh,
    _ => sheets[0].id,
  };
  let key = (app.s.revision, id, on);
  if app.sheet_cache.as_ref().map(|c| c.0) != Some(key) {
    let plot = fox_engine::model::drawing::render_sheet(d, &app.s.design, &app.s.model, on);
    app.sheet_cache = Some((key, plot));
  }
  let (w, h) = match app.sheet_cache.as_ref().map(|c| &c.1) {
    Some(Ok(plot)) => (plot.width, plot.height),
    _ => d.sheet.size(),
  };

  let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
  app.canvas = rect;
  let p = ui.painter_at(rect);
  p.rect_filled(rect, 0.0, BACKDROP);

  // zoom about the pointer, pan with the right or middle button
  let (mut zoom, mut pan) = app.sheet_view;
  if resp.hovered() {
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll != 0.0 {
      let f = (scroll * 0.002).exp();
      let new = (zoom * f).clamp(0.2, 40.0);
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
  if resp.double_clicked() {
    zoom = 1.0;
    pan = egui::Vec2::ZERO;
  }
  app.sheet_view = (zoom, pan);

  // the sheet fits the canvas at zoom 1, below the toolbar and above the status line
  let fit = ((rect.width() - 80.0) / w as f32).min((rect.height() - 90.0) / h as f32).max(0.05);
  let k = fit * zoom;
  let size = vec2(w as f32 * k, h as f32 * k);
  let sheet = Rect::from_center_size(rect.center() + pan + vec2(0.0, -8.0), size);
  let to = |q: [f64; 2]| pos2(sheet.left() + q[0] as f32 * k, sheet.bottom() - q[1] as f32 * k);
  p.rect_filled(sheet.translate(vec2(2.0, 3.0)), 0.0, Color32::from_black_alpha(40));
  p.rect_filled(sheet, 0.0, Color32::WHITE);

  match app.sheet_cache.as_ref().map(|c| &c.1) {
    Some(Ok(plot)) => {
      for item in &plot.items {
        draw_item(&p, item, k, &to);
      }
    }
    Some(Err(e)) => {
      p.text(sheet.center(), egui::Align2::CENTER_CENTER, e, egui::FontId::proportional(14.0), theme::ERROR);
    }
    None => {}
  }

  tabs(app, ui, rect, id, &sheets, on);
}

/// The sheet tabs under the drawing, as Fusion's: click one to show it, right-click to delete
/// it, + adds a sheet.
fn tabs(app: &mut App, ui: &egui::Ui, rect: Rect, id: u32, sheets: &[fox_engine::model::drawing::Sheet], on: u32) {
  let p = ui.painter_at(rect);
  let font = egui::FontId::proportional(12.0);
  let mut x = rect.left() + 12.0;
  let y = rect.bottom() - 30.0;
  for sh in sheets {
    let w = (p.layout_no_wrap(sh.name.clone(), font.clone(), theme::TEXT).size().x + 24.0).max(80.0);
    let tab = Rect::from_min_size(pos2(x, y), vec2(w, 22.0));
    let r = ui.interact(tab, ui.id().with(("sheet-tab", id, sh.id)), Sense::click());
    let fill = if sh.id == on { Color32::WHITE } else { Color32::from_rgb(232, 235, 239) };
    p.rect(tab, 2.0, fill, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    p.text(tab.center(), egui::Align2::CENTER_CENTER, &sh.name, font.clone(), theme::TEXT);
    if r.clicked() {
      app.s.drawing_sheet = Some((id, sh.id));
    }
    if sheets.len() > 1 {
      r.context_menu(|ui| {
        if ui.button(format!("Delete {}", sh.name)).clicked() {
          app.run("drawing.remove_sheet", json!({"drawing": id, "sheet": sh.id}));
          ui.close();
        }
      });
    }
    x += w + 4.0;
  }
  let plus = Rect::from_min_size(pos2(x, y), vec2(22.0, 22.0));
  let r = ui.interact(plus, ui.id().with(("sheet-add", id)), Sense::click()).on_hover_text("New sheet");
  p.rect(plus, 2.0, BACKDROP, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  p.text(plus.center(), egui::Align2::CENTER_CENTER, "+", font, theme::TEXT);
  if r.clicked() {
    app.run("drawing.add_sheet", json!({"drawing": id}));
  }
}

const SIZES: &[(&str, &str)] =
  &[("a3", "A3"), ("a4", "A4"), ("a2", "A2"), ("a1", "A1"), ("a0", "A0"), ("letter", "Letter"), ("tabloid", "Tabloid")];

pub static DRAWING_NEW_SHEET: Form = Form {
  title: "NEW SHEET",
  name: "New Sheet",
  hint: "Add a sheet to the drawing; new views go on it",
  icon: Icon::SheetSettings,
  cmd: "drawing.add_sheet",
  fixed: &[],
  inputs: &[
    Input::Text { key: "name", label: "Name", default: "" },
    Input::Choice { key: "size", label: "Size", options: SIZES },
    Input::Text { key: "scale", label: "Scale (1:2, empty = drawing's)", default: "" },
  ],
  picks_model: false,
};

pub static DRAWING_EDIT_SHEET: Form = Form {
  title: "EDIT SHEET",
  name: "Edit Sheet",
  hint: "Rename the sheet on show, or change its size or scale",
  icon: Icon::SheetSettings,
  cmd: "drawing.set_sheet",
  fixed: &[],
  inputs: &[
    Input::Text { key: "name", label: "Name", default: "" },
    Input::Choice { key: "size", label: "Size", options: SIZES },
    Input::Text { key: "scale", label: "Scale (1:2, empty = drawing's)", default: "" },
  ],
  picks_model: false,
};

const AXES: &[(&str, &str)] = &[("horizontal", "Horizontal"), ("vertical", "Vertical")];

pub static DRAWING_BASELINE: Form = Form {
  title: "BASELINE DIMENSION",
  name: "Baseline Dimension",
  hint: "Points measured from one origin, the dimension lines stacked",
  icon: Icon::OrdinateDim,
  cmd: "drawing.baseline",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "origin", label: "Origin (model)", default: ["0", "0", "0"] },
    Input::Text { key: "points", label: "Points (x y z; …)", default: "" },
    Input::Choice { key: "orientation", label: "Direction", options: AXES },
    Input::Expr { key: "offset", label: "Offset (sheet mm)", default: "10" },
    Input::Expr { key: "spacing", label: "Spacing (sheet mm)", default: "8" },
  ],
  picks_model: false,
};

pub static DRAWING_CHAIN: Form = Form {
  title: "CHAIN DIMENSION",
  name: "Chain Dimension",
  hint: "Distances between consecutive points, on one line",
  icon: Icon::Dimension,
  cmd: "drawing.chain",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Text { key: "points", label: "Points (x y z; …)", default: "" },
    Input::Choice { key: "orientation", label: "Direction", options: AXES },
    Input::Expr { key: "offset", label: "Offset (sheet mm)", default: "10" },
  ],
  picks_model: false,
};

pub static DRAWING_WELD: Form = Form {
  title: "WELD SYMBOL",
  name: "Weld Symbol",
  hint: "A welding symbol on a joint (AWS A2.4 or ISO 2553, per the drawing's standard)",
  icon: Icon::Fillet,
  cmd: "drawing.weld",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "point", label: "Joint point (model)", default: ["0", "0", "0"] },
    Input::Choice {
      key: "kind",
      label: "Weld",
      options: &[
        ("fillet", "Fillet"),
        ("square", "Square (I) groove"),
        ("v", "V groove"),
        ("bevel", "Bevel groove"),
        ("u", "U groove"),
        ("j", "J groove"),
        ("plug", "Plug or slot"),
        ("spot", "Spot"),
        ("seam", "Seam"),
      ],
    },
    Input::Choice {
      key: "side",
      label: "Side",
      options: &[("arrow", "Arrow side"), ("other", "Other side"), ("both", "Both sides")],
    },
    Input::Text { key: "size", label: "Size", default: "6" },
    Input::Text { key: "length", label: "Length(-pitch)", default: "" },
    Input::Check { key: "all_around", label: "All around" },
    Input::Check { key: "field", label: "Field weld" },
    Input::Text { key: "tail", label: "Tail (process)", default: "" },
    Input::Expr { key: "offset_x", label: "Offset X (sheet mm)", default: "12" },
    Input::Expr { key: "offset_y", label: "Offset Y (sheet mm)", default: "12" },
  ],
  picks_model: false,
};

pub static DRAWING_DATUM_TARGET: Form = Form {
  title: "DATUM TARGET",
  name: "Datum Target",
  hint: "A datum target point (area 0) or area on a feature",
  icon: Icon::DatumId,
  cmd: "drawing.datum_target",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "point", label: "Target point (model)", default: ["0", "0", "0"] },
    Input::Text { key: "label", label: "Target", default: "A1" },
    Input::Expr { key: "area", label: "Area diameter (model mm)", default: "0" },
    Input::Expr { key: "offset_x", label: "Offset X (sheet mm)", default: "12" },
    Input::Expr { key: "offset_y", label: "Offset Y (sheet mm)", default: "12" },
  ],
  picks_model: false,
};

pub static DRAWING_REVISION: Form = Form {
  title: "REVISION",
  name: "Revision Table",
  hint: "Add a row to the revision table (the next letter when left empty)",
  icon: Icon::TitleBlock,
  cmd: "drawing.revision",
  fixed: &[],
  inputs: &[
    Input::Text { key: "rev", label: "Revision", default: "" },
    Input::Text { key: "description", label: "Description", default: "" },
    Input::Text { key: "date", label: "Date", default: "" },
    Input::Text { key: "by", label: "Approved by", default: "" },
  ],
  picks_model: false,
};

fn draw_item(p: &egui::Painter, item: &Item, k: f32, to: &impl Fn([f64; 2]) -> Pos2) {
  let ink = Color32::from_rgb(20, 24, 30);
  match item {
    Item::Line { pen, points } => {
      let pts: Vec<Pos2> = points.iter().map(|q| to(*q)).collect();
      let mm = match pen {
        Pen::Visible => 0.5,
        Pen::Border => 0.7,
        Pen::Hidden => 0.3,
        Pen::Thin | Pen::Center => 0.18,
      };
      let stroke = Stroke::new((mm * k).max(0.6), if *pen == Pen::Thin { Color32::from_gray(60) } else { ink });
      match pen {
        Pen::Hidden => p.extend(Shape::dashed_line(&pts, stroke, 3.0 * k, 1.5 * k)),
        Pen::Center => p.extend(Shape::dashed_line(&pts, stroke, 8.0 * k, 3.0 * k)),
        _ => {
          p.add(Shape::line(pts, stroke));
        }
      }
    }
    Item::Fill { points } => {
      p.add(Shape::convex_polygon(points.iter().map(|q| to(*q)).collect(), ink, Stroke::NONE));
    }
    Item::Text { at, size, text, anchor } => {
      // plot text sizes are cap heights in mm; anchored at the baseline
      let font = egui::FontId::proportional((*size as f32 * k * 1.4).max(4.0));
      let align = match anchor {
        Anchor::Start => egui::Align2::LEFT_BOTTOM,
        Anchor::Middle => egui::Align2::CENTER_BOTTOM,
        Anchor::End => egui::Align2::RIGHT_BOTTOM,
      };
      p.text(to(*at) + vec2(0.0, *size as f32 * k * 0.3), align, text, font, ink);
    }
    Item::RotatedText { at, size, text, angle } => {
      let font = egui::FontId::proportional((*size as f32 * k * 1.4).max(4.0));
      let galley = p.layout_no_wrap(text.clone(), font, ink);
      let a = -(*angle as f32).to_radians();
      let half = galley.size() / 2.0;
      // rotate the galley's centre offset so the text stays centred on `at`
      let (sn, cs) = a.sin_cos();
      let off = vec2(half.x * cs - half.y * sn, half.x * sn + half.y * cs);
      p.add(egui::epaint::TextShape::new(to(*at) - off, galley, ink).with_angle(a));
    }
  }
}
