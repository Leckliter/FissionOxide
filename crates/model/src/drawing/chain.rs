//! Baseline and chain dimensions: rows of linear dimensions along a view's horizontal or
//! vertical, built from the same [`linear_dim`] as single dimensions. Values are true (model)
//! lengths, so they read right in broken views too.

use super::{DimOrientation, Projected, Standard, linear_dim, sheet_pt};
use crate::plot::Plot;

/// Default sheet mm between the stacked lines of a baseline dimension.
pub(super) fn spacing() -> f64 {
  8.0
}

/// A model point in a view: (sheet point, view coordinates).
fn mark(pv: &Projected, scale: f64, p: [f64; 3]) -> ([f64; 2], [f64; 2]) {
  let v = pv.to_view(p.into());
  (sheet_pt(pv, scale, v), v)
}

/// The side a horizontal (vertical) row sits on: the sheet's y (−x) away from the points.
fn level(p: [f64; 2], o: DimOrientation) -> f64 {
  if o == DimOrientation::Vertical { -p[0] } else { p[1] }
}

/// The offset that puts the dimension line between `a` and `b` at `line` (in [`level`] units).
fn offset_to(a: [f64; 2], b: [f64; 2], o: DimOrientation, line: f64, outward: bool) -> f64 {
  let (la, lb) = (level(a, o), level(b, o));
  if outward { line - la.max(lb) } else { line - la.min(lb) }
}

/// The level of the first row: `offset` past the outermost point.
fn first_line(marks: &[[f64; 2]], o: DimOrientation, offset: f64) -> f64 {
  let ls = marks.iter().map(|p| level(*p, o));
  if offset < 0.0 { ls.fold(f64::INFINITY, f64::min) + offset } else { ls.fold(f64::NEG_INFINITY, f64::max) + offset }
}

/// Baseline dimensions from `origin` to each point, nearest first, stacked `spacing` apart.
#[allow(clippy::too_many_arguments)]
pub(super) fn baseline(
  plot: &mut Plot,
  std: Standard,
  pv: &Projected,
  scale: f64,
  origin: [f64; 3],
  points: &[[f64; 3]],
  o: DimOrientation,
  offset: f64,
  spacing: f64,
) {
  let i = if o == DimOrientation::Vertical { 1 } else { 0 };
  let (so, vo) = mark(pv, scale, origin);
  let mut marks: Vec<([f64; 2], f64)> = points
    .iter()
    .map(|p| mark(pv, scale, *p))
    .map(|(s, v)| (s, (v[i] - vo[i]).abs()))
    .filter(|(_, d)| *d > 1e-9)
    .collect();
  marks.sort_by(|a, b| a.1.total_cmp(&b.1));
  marks.dedup_by(|a, b| (a.1 - b.1).abs() < 1e-9);
  let all: Vec<[f64; 2]> = std::iter::once(so).chain(marks.iter().map(|m| m.0)).collect();
  let start = first_line(&all, o, offset);
  let step = if offset < 0.0 { -spacing.abs() } else { spacing.abs() };
  for (k, (p, value)) in marks.iter().enumerate() {
    let line = start + step * k as f64;
    let off = offset_to(so, *p, o, line, offset >= 0.0);
    linear_dim(plot, so, *p, o, off, &std.number(*value));
  }
}

/// Chain dimensions between consecutive points (in order along the view's axis), on one line.
#[allow(clippy::too_many_arguments)]
pub(super) fn chain(
  plot: &mut Plot,
  std: Standard,
  pv: &Projected,
  scale: f64,
  points: &[[f64; 3]],
  o: DimOrientation,
  offset: f64,
) {
  let i = if o == DimOrientation::Vertical { 1 } else { 0 };
  let mut marks: Vec<([f64; 2], [f64; 2])> = points.iter().map(|p| mark(pv, scale, *p)).collect();
  marks.sort_by(|a, b| a.1[i].total_cmp(&b.1[i]));
  marks.dedup_by(|a, b| (a.1[i] - b.1[i]).abs() < 1e-9);
  let sheet: Vec<[f64; 2]> = marks.iter().map(|m| m.0).collect();
  let line = first_line(&sheet, o, offset);
  for w in marks.windows(2) {
    let ((a, va), (b, vb)) = (w[0], w[1]);
    let off = offset_to(a, b, o, line, offset >= 0.0);
    linear_dim(plot, a, b, o, off, &std.number(vb[i] - va[i]));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::plot::Item;

  /// The dimension lines (between two arrowheads) of a plot: horizontal segments, by length.
  fn dim_lines(plot: &Plot) -> Vec<[[f64; 2]; 2]> {
    let mut out = Vec::new();
    for (k, it) in plot.items.iter().enumerate() {
      // linear_dim pushes: extension, extension, dimension line, arrow, arrow, text
      if let (Item::Line { points, .. }, Some(Item::Fill { .. })) = (it, plot.items.get(k + 1))
        && points.len() == 2
      {
        out.push([points[0], points[1]]);
      }
    }
    out
  }

  #[test]
  fn rows_line_up() {
    // the dimension lines of one chain share a level; baseline lines step out by the spacing
    let mut plot = Plot::default();
    let pts = [[0.0, 0.0], [10.0, 5.0], [30.0, 0.0], [45.0, 2.0]];
    let line = first_line(&pts, DimOrientation::Horizontal, 10.0);
    assert_eq!(line, 15.0);
    for w in pts.windows(2) {
      let off = offset_to(w[0], w[1], DimOrientation::Horizontal, line, true);
      linear_dim(&mut plot, w[0], w[1], DimOrientation::Horizontal, off, "x");
    }
    let lines = dim_lines(&plot);
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| (l[0][1] - 15.0).abs() < 1e-9 && (l[1][1] - 15.0).abs() < 1e-9), "{lines:?}");
    // below, for a negative offset; to the left for vertical rows
    assert_eq!(first_line(&pts, DimOrientation::Horizontal, -10.0), -10.0);
    assert_eq!(first_line(&pts, DimOrientation::Vertical, 10.0), 10.0);
    assert_eq!(level([3.0, 4.0], DimOrientation::Vertical), -3.0);
  }
}
