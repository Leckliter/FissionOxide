//! The FissionOxide fox, painted from its facets in `fox-brand` so it stays sharp at any size.

use egui::{Painter, Pos2, Rect, Shape, Stroke, pos2};
use fox_brand::logo::{FACETS, SIZE};

use crate::theme::color;

/// Paints the fox centred in `rect`, as large as fits.
pub fn paint(painter: &Painter, rect: Rect) {
  let s = (rect.width() / SIZE.0).min(rect.height() / SIZE.1);
  let origin = rect.center() - egui::vec2(SIZE.0, SIZE.1) * s / 2.0;
  for f in FACETS {
    let pts: Vec<Pos2> = f.points.iter().map(|&(x, y)| pos2(origin.x + x * s, origin.y + y * s)).collect();
    let c = color(f.color);
    // a hairline in the facet's colour hides the anti-aliasing seams between facets
    painter.add(Shape::convex_polygon(pts, c, Stroke::new(0.6, c)));
  }
}
