//! The Drawing workspace's canvas: the open drawing's sheet, drawn from the same vector plot the
//! PDF / DXF / SVG exports are written from. Wheel zooms, right- or middle-drag pans,
//! double-click fits the sheet.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use fox_engine::model::plot::{Anchor, Item, Pen};

use crate::{App, theme};

const BACKDROP: Color32 = Color32::from_rgb(214, 218, 224);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  let Some(id) = app.drawing else { return };
  let Some(d) = app.s.design.drawing(id) else {
    app.drawing = None;
    return;
  };
  let key = (app.s.revision, id);
  if app.sheet_cache.as_ref().map(|c| c.0) != Some(key) {
    let plot = fox_engine::model::drawing::render(d, &app.s.design, &app.s.model);
    app.sheet_cache = Some((key, plot));
  }
  let (w, h) = d.sheet.size();
  let name = d.name.clone();

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

  // the sheet's name, like the sheet tab under Fusion's drawing
  let tab = Rect::from_min_size(pos2(rect.left() + 12.0, rect.bottom() - 30.0), vec2(120.0, 22.0));
  p.rect(tab, 2.0, Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  p.text(tab.center(), egui::Align2::CENTER_CENTER, &name, egui::FontId::proportional(12.0), theme::TEXT);
}

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
