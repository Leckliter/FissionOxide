//! Welding symbols (AWS A2.4 / ISO 2553 basics), datum targets and composite feature control
//! frames.
//!
//! A welding symbol is an arrow to the joint, a horizontal reference line and the weld symbol on
//! it with its size to the left and its length (or length-pitch) to the right; a circle at the
//! elbow means all around, a flag a field weld, a fork at the far end carries the tail (process,
//! specification). AWS A2.4 (the ASME drawings) puts arrow-side welds below the reference line
//! and other-side welds above it. ISO 2553 (system A) adds a dashed identification line under
//! the reference line: welds on the arrow side stand on the solid line, other-side welds hang
//! under the dashed one, and a symmetric (both sides) weld has no dashed line.

use serde::{Deserialize, Serialize};

use super::annotate::{Characteristic, MaterialCondition, Standard};
use super::{TEXT, arrow};
use crate::plot::{Anchor, Item, Pen, Plot, text_width};

/// The weld symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldKind {
  Fillet,
  /// Square (ISO: I) groove.
  Square,
  V,
  Bevel,
  U,
  J,
  /// Plug or slot.
  Plug,
  Spot,
  Seam,
}

impl WeldKind {
  pub const ALL: [WeldKind; 9] = [
    WeldKind::Fillet,
    WeldKind::Square,
    WeldKind::V,
    WeldKind::Bevel,
    WeldKind::U,
    WeldKind::J,
    WeldKind::Plug,
    WeldKind::Spot,
    WeldKind::Seam,
  ];

  pub fn key(self) -> &'static str {
    match self {
      WeldKind::Fillet => "fillet",
      WeldKind::Square => "square",
      WeldKind::V => "v",
      WeldKind::Bevel => "bevel",
      WeldKind::U => "u",
      WeldKind::J => "j",
      WeldKind::Plug => "plug",
      WeldKind::Spot => "spot",
      WeldKind::Seam => "seam",
    }
  }

  pub fn parse(s: &str) -> Option<WeldKind> {
    let s = s.to_ascii_lowercase();
    let s = match s.trim().trim_end_matches("_groove").trim_end_matches(" groove") {
      "i" | "butt" => "square",
      "slot" => "plug",
      o => o,
    }
    .to_string();
    WeldKind::ALL.into_iter().find(|k| k.key() == s)
  }
}

/// Which side of the joint the weld is on, seen from the arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WeldSide {
  #[default]
  Arrow,
  Other,
  Both,
}

impl WeldSide {
  pub fn parse(s: &str) -> Option<WeldSide> {
    Some(match s.to_ascii_lowercase().as_str() {
      "arrow" | "arrow_side" | "near" => WeldSide::Arrow,
      "other" | "other_side" | "far" => WeldSide::Other,
      "both" | "both_sides" | "symmetric" => WeldSide::Both,
      _ => return None,
    })
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeldSymbol {
  pub kind: WeldKind,
  #[serde(default)]
  pub side: WeldSide,
  /// Left of the symbol: leg or throat ("6", "a5", "z7"), groove depth, spot diameter.
  #[serde(default)]
  pub size: String,
  /// Right of the symbol: length, or length-pitch of an intermittent weld ("50-150").
  #[serde(default)]
  pub length: String,
  /// Weld all around (a circle at the elbow).
  #[serde(default)]
  pub all_around: bool,
  /// Weld in the field, on site (a flag at the elbow).
  #[serde(default)]
  pub field: bool,
  /// In the tail: process, specification ("GMAW", "135").
  #[serde(default)]
  pub tail: String,
}

/// One tier of a feature control frame (the lower one of a composite frame).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameTier {
  pub tolerance: f64,
  #[serde(default)]
  pub diameter: bool,
  #[serde(default)]
  pub modifier: Option<MaterialCondition>,
  #[serde(default)]
  pub datums: Vec<String>,
}

impl FrameTier {
  /// The lower tier of a composite frame must refine the upper: a tighter tolerance and the
  /// upper's datums, in order, or fewer of them.
  pub fn check_composite(&self, ch: Characteristic, upper: &FrameTier) -> Result<(), String> {
    if !matches!(ch, Characteristic::Position | Characteristic::ProfileOfLine | Characteristic::ProfileOfSurface) {
      return Err("only position and profile tolerances have composite frames".into());
    }
    if !(self.tolerance > 0.0 && self.tolerance < upper.tolerance) {
      return Err("the lower tier needs a positive tolerance smaller than the upper's".into());
    }
    if self.datums.len() > upper.datums.len() || self.datums.iter().zip(&upper.datums).any(|(a, b)| a != b) {
      return Err("the lower tier's datums repeat the upper's, in order (or fewer of them)".into());
    }
    Ok(())
  }
}

const SMALL: f64 = 2.5;
/// Height of a weld symbol (sheet mm) and the gap of ISO's dashed identification line.
const SYM: f64 = 3.0;
const DASH_GAP: f64 = 1.2;
/// Frame cell height, as in [`super::annotate`].
const CELL: f64 = 2.0 * TEXT;

fn line(plot: &mut Plot, points: Vec<[f64; 2]>) {
  plot.items.push(Item::Line { pen: Pen::Thin, points });
}

fn circle(c: [f64; 2], r: f64, n: usize) -> Vec<[f64; 2]> {
  arc(c, r, 0.0, std::f64::consts::TAU, n)
}

fn arc(c: [f64; 2], r: f64, from: f64, sweep: f64, n: usize) -> Vec<[f64; 2]> {
  (0..=n).map(|i| from + sweep * i as f64 / n as f64).map(|t| [c[0] + r * t.cos(), c[1] + r * t.sin()]).collect()
}

/// A weld symbol standing on (s = 1) or hanging under (s = −1) the line at `base`, centred on x.
fn weld_shape(plot: &mut Plot, kind: WeldKind, x: f64, base: f64, s: f64) {
  let p = |u: f64, v: f64| [x + u, base + s * v];
  let h = SYM;
  match kind {
    // the vertical leg always on the left
    WeldKind::Fillet => line(plot, vec![p(-1.5, 0.0), p(-1.5, h), p(1.5, 0.0)]),
    WeldKind::Square => {
      line(plot, vec![p(-0.8, 0.0), p(-0.8, h)]);
      line(plot, vec![p(0.8, 0.0), p(0.8, h)]);
    }
    WeldKind::V => line(plot, vec![p(-1.7, h), p(0.0, 0.0), p(1.7, h)]),
    WeldKind::Bevel => {
      line(plot, vec![p(-1.2, 0.0), p(-1.2, h)]);
      line(plot, vec![p(-1.2, 0.0), p(1.6, h)]);
    }
    WeldKind::U => {
      line(plot, vec![p(0.0, 0.0), p(0.0, 1.2)]);
      let a = arc([0.0, 2.7], 1.5, std::f64::consts::PI, std::f64::consts::PI, 16);
      line(plot, a.iter().map(|q| p(q[0], q[1])).collect());
    }
    WeldKind::J => {
      line(plot, vec![p(-1.2, 0.0), p(-1.2, h)]);
      let a = arc([-1.2, h], 2.4, 1.5 * std::f64::consts::PI, 0.5 * std::f64::consts::PI, 12);
      line(plot, a.iter().map(|q| p(q[0], q[1])).collect());
    }
    WeldKind::Plug => line(plot, vec![p(-1.8, 0.0), p(-1.8, 2.0), p(1.8, 2.0), p(1.8, 0.0)]),
    WeldKind::Spot => {
      line(plot, circle(p(0.0, 1.5), 1.5, 24));
    }
    WeldKind::Seam => {
      line(plot, circle(p(0.0, 1.5), 1.5, 24));
      line(plot, vec![p(-2.3, 1.0), p(2.3, 1.0)]);
      line(plot, vec![p(-2.3, 2.0), p(2.3, 2.0)]);
    }
  }
}

/// A welding symbol: the arrow on the joint at `anchor`, the elbow `offset` mm from it.
pub(super) fn weld(plot: &mut Plot, std: Standard, anchor: [f64; 2], offset: [f64; 2], w: &WeldSymbol) {
  let elbow = [anchor[0] + offset[0], anchor[1] + offset[1]];
  let d = [anchor[0] - elbow[0], anchor[1] - elbow[1]];
  let l = d[0].hypot(d[1]);
  if l > 1e-6 {
    line(plot, vec![elbow, anchor]);
    plot.items.push(arrow(anchor, [d[0] / l, d[1] / l]));
  }
  // the reference line runs away from the arrow: size, symbol, length along it
  let dir = if offset[0] >= 0.0 { 1.0 } else { -1.0 };
  let (sw, lw) = (text_width(&w.size, SMALL), text_width(&w.length, SMALL));
  let len = (3.0 + sw + 1.0 + 5.0 + 1.0 + lw + 3.0).max(20.0);
  let (x0, x1) = if dir > 0.0 { (elbow[0], elbow[0] + len) } else { (elbow[0] - len, elbow[0]) };
  let y = elbow[1];
  plot.items.push(Item::Line { pen: Pen::Thin, points: vec![[x0, y], [x1, y]] });
  let cx = (x0 + x1) / 2.0 + (sw - lw) / 2.0;
  // (base of the symbol, which way it points) for each side the weld is on
  let iso = std == Standard::Iso;
  let mut places = Vec::new();
  let (arrow_side, other_side) = if iso { ((y, 1.0), (y - DASH_GAP, -1.0)) } else { ((y, -1.0), (y, 1.0)) };
  match w.side {
    WeldSide::Arrow => places.push(arrow_side),
    WeldSide::Other => places.push(other_side),
    // a symmetric weld straddles the plain reference line
    WeldSide::Both => places.extend([(y, 1.0), (y, -1.0)]),
  }
  if iso && w.side != WeldSide::Both {
    let yd = y - DASH_GAP;
    plot.items.push(Item::Line { pen: Pen::Hidden, points: vec![[x0, yd], [x1, yd]] });
  }
  for (base, s) in places {
    weld_shape(plot, w.kind, cx, base, s);
    let mid = base + s * SYM / 2.0 - SMALL / 2.0;
    if !w.size.is_empty() {
      plot.items.push(Item::Text { at: [cx - 3.0, mid], size: SMALL, text: w.size.clone(), anchor: Anchor::End });
    }
    if !w.length.is_empty() {
      plot.items.push(Item::Text { at: [cx + 3.0, mid], size: SMALL, text: w.length.clone(), anchor: Anchor::Start });
    }
  }
  if w.all_around {
    line(plot, circle(elbow, 1.6, 24));
  }
  if w.field {
    // a flag on a staff at the elbow, pointing to the tail
    let top = [elbow[0], elbow[1] + 6.0];
    line(plot, vec![elbow, top]);
    plot.items.push(Item::Fill { points: vec![top, [top[0] + dir * 3.5, top[1] - 1.2], [top[0], top[1] - 2.4]] });
  }
  if !w.tail.is_empty() {
    let end = [if dir > 0.0 { x1 } else { x0 }, y];
    line(plot, vec![[end[0] + dir * 2.5, y + 2.5], end, [end[0] + dir * 2.5, y - 2.5]]);
    let anchor = if dir > 0.0 { Anchor::Start } else { Anchor::End };
    plot.items.push(Item::Text {
      at: [end[0] + dir * 3.5, y - SMALL / 2.0],
      size: SMALL,
      text: w.tail.clone(),
      anchor,
    });
  }
}

/// A datum target: a circle split across, an area target's diameter above and the target
/// ("A1") below, on a leader to an × (a point target) or to the area's outline; `area` is the
/// area's diameter in model mm (0 for a point), `k` sheet mm per model mm.
#[allow(clippy::too_many_arguments)]
pub(super) fn datum_target(
  plot: &mut Plot,
  std: Standard,
  anchor: [f64; 2],
  offset: [f64; 2],
  label: &str,
  area: f64,
  k: f64,
) {
  let c = [anchor[0] + offset[0], anchor[1] + offset[1]];
  let size = (area > 0.0).then(|| format!("Ø{}", std.number(area)));
  let wide = size.as_deref().map_or(0.0, |t| text_width(t, SMALL));
  let r = ((text_width(label, SMALL).max(wide)) / 2.0 + 1.5).max(4.5);
  line(plot, circle(c, r, 48));
  line(plot, vec![[c[0] - r, c[1]], [c[0] + r, c[1]]]);
  plot.items.push(Item::Text {
    at: [c[0], c[1] - SMALL - 0.8],
    size: SMALL,
    text: label.into(),
    anchor: Anchor::Middle,
  });
  if let Some(text) = size {
    plot.items.push(Item::Text { at: [c[0], c[1] + 0.8], size: SMALL, text, anchor: Anchor::Middle });
    plot.items.push(Item::Line { pen: Pen::Center, points: circle(anchor, area * k / 2.0, 48) });
  }
  let d = [anchor[0] - c[0], anchor[1] - c[1]];
  let l = d[0].hypot(d[1]);
  if l > r {
    line(plot, vec![[c[0] + d[0] / l * r, c[1] + d[1] / l * r], anchor]);
  }
  if area <= 0.0 {
    let x = 1.5;
    line(plot, vec![[anchor[0] - x, anchor[1] - x], [anchor[0] + x, anchor[1] + x]]);
    line(plot, vec![[anchor[0] - x, anchor[1] + x], [anchor[0] + x, anchor[1] - x]]);
  }
}

/// Cell widths of a frame tier: symbol, tolerance, datums (as [`super::annotate`] draws them).
fn widths(std: Standard, t: &FrameTier) -> Vec<f64> {
  let tol = format!("{}{}", if t.diameter { "Ø" } else { "" }, std.number(t.tolerance));
  let mod_w = if t.modifier.is_some() { 5.0 } else { 0.0 };
  let mut w = vec![CELL, (text_width(&tol, TEXT) + 3.0 + mod_w).max(CELL)];
  w.extend(t.datums.iter().map(|d| (text_width(d, TEXT) + 3.0).max(CELL)));
  w
}

/// The lower tier of a composite frame, under the upper one drawn by
/// [`super::annotate::feature_control`]; the symbol cell spans both tiers.
pub(super) fn composite_lower(
  plot: &mut Plot,
  std: Standard,
  anchor: [f64; 2],
  offset: [f64; 2],
  upper: &FrameTier,
  lower: &FrameTier,
) {
  let at = [anchor[0] + offset[0], anchor[1] + offset[1]];
  let total: f64 = widths(std, upper).iter().sum();
  let x0 = if offset[0] >= 0.0 { at[0] } else { at[0] - total };
  let (y1, y0) = (at[1] - CELL / 2.0, at[1] - 1.5 * CELL);
  let w = widths(std, lower);
  let lower_total: f64 = w.iter().sum();
  // the symbol cell grows down; the lower tier's cells start after it
  line(plot, vec![[x0, y1], [x0, y0], [x0 + lower_total, y0], [x0 + lower_total, y1]]);
  let mut x = x0;
  for cw in &w[..w.len() - 1] {
    x += cw;
    line(plot, vec![[x, y0], [x, y1]]);
  }
  let mid = (y0 + y1) / 2.0;
  let tol = format!("{}{}", if lower.diameter { "Ø" } else { "" }, std.number(lower.tolerance));
  plot.items.push(Item::Text {
    at: [x0 + CELL + 1.5, y0 + (CELL - TEXT) / 2.0],
    size: TEXT,
    text: tol,
    anchor: Anchor::Start,
  });
  if let Some(m) = lower.modifier {
    let c = [x0 + CELL + w[1] - 3.5, mid];
    line(plot, circle(c, 2.0, 24));
    let letter = if m == MaterialCondition::Mmc { "M" } else { "L" };
    plot.items.push(Item::Text { at: [c[0], c[1] - 1.25], size: 2.5, text: letter.into(), anchor: Anchor::Middle });
  }
  let mut x = x0 + CELL + w[1];
  for (d, cw) in lower.datums.iter().zip(&w[2..]) {
    plot.items.push(Item::Text {
      at: [x + cw / 2.0, mid - TEXT / 2.0],
      size: TEXT,
      text: d.clone(),
      anchor: Anchor::Middle,
    });
    x += cw;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sym(kind: WeldKind, side: WeldSide) -> WeldSymbol {
    WeldSymbol {
      kind,
      side,
      size: "6".into(),
      length: "50-150".into(),
      all_around: false,
      field: false,
      tail: String::new(),
    }
  }

  /// The y range of the thin strokes other than the reference line and the arrow's leader.
  fn symbol_y(plot: &Plot, y: f64) -> (f64, f64) {
    plot
      .items
      .iter()
      .filter_map(|i| if let Item::Line { pen: Pen::Thin, points } = i { Some(points) } else { None })
      .filter(|p| !(p.len() == 2 && (p[0][1] - y).abs() < 1e-9 && (p[1][1] - y).abs() < 1e-9))
      .skip(1)
      .flatten()
      .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])))
  }

  #[test]
  fn arrow_side_is_below_in_aws_and_on_the_solid_line_in_iso() {
    for kind in WeldKind::ALL {
      let mut aws = Plot::default();
      weld(&mut aws, Standard::Asme, [0.0, 0.0], [10.0, 10.0], &sym(kind, WeldSide::Arrow));
      let (lo, hi) = symbol_y(&aws, 10.0);
      assert!(hi <= 10.0 + 1e-9 && lo < 10.0, "{kind:?} {lo} {hi}");
      // no dashed line in AWS
      assert!(!aws.items.iter().any(|i| matches!(i, Item::Line { pen: Pen::Hidden, .. })));

      let mut other = Plot::default();
      weld(&mut other, Standard::Asme, [0.0, 0.0], [10.0, 10.0], &sym(kind, WeldSide::Other));
      let (lo, _) = symbol_y(&other, 10.0);
      assert!(lo >= 10.0 - 1e-9, "{kind:?}");

      let mut iso = Plot::default();
      weld(&mut iso, Standard::Iso, [0.0, 0.0], [10.0, 10.0], &sym(kind, WeldSide::Arrow));
      let (lo, _) = symbol_y(&iso, 10.0);
      assert!(lo >= 10.0 - 1e-9, "{kind:?}");
      assert_eq!(iso.items.iter().filter(|i| matches!(i, Item::Line { pen: Pen::Hidden, .. })).count(), 1);

      let mut iso_other = Plot::default();
      weld(&mut iso_other, Standard::Iso, [0.0, 0.0], [10.0, 10.0], &sym(kind, WeldSide::Other));
      let (_, hi) = symbol_y(&iso_other, 10.0);
      assert!(hi <= 10.0 - DASH_GAP + 1e-9, "{kind:?}");
    }
    // a symmetric weld: on both sides, no identification line
    let mut both = Plot::default();
    weld(&mut both, Standard::Iso, [0.0, 0.0], [10.0, 10.0], &sym(WeldKind::V, WeldSide::Both));
    assert!(!both.items.iter().any(|i| matches!(i, Item::Line { pen: Pen::Hidden, .. })));
    assert_eq!(both.texts(), ["6", "50-150", "6", "50-150"]);
  }

  #[test]
  fn size_left_length_right_and_the_extras() {
    let mut plot = Plot::default();
    let w = WeldSymbol { all_around: true, field: true, tail: "GMAW".into(), ..sym(WeldKind::Fillet, WeldSide::Arrow) };
    // the arrow to the left: the reference line runs left of the elbow
    weld(&mut plot, Standard::Asme, [0.0, 0.0], [-10.0, 10.0], &w);
    let x = |s: &str| {
      plot
        .items
        .iter()
        .find_map(|i| if let Item::Text { at, text, .. } = i { (text == s).then_some(at[0]) } else { None })
    };
    assert!(x("6").unwrap() < x("50-150").unwrap());
    // the tail is at the far (left) end
    assert!(x("GMAW").unwrap() < x("6").unwrap());
    // the arrowhead and the field flag
    assert_eq!(plot.items.iter().filter(|i| matches!(i, Item::Fill { .. })).count(), 2);
    // the all-around circle centred on the elbow
    let ring = plot.items.iter().any(|i| {
      matches!(i, Item::Line { points, .. } if points.len() == 25
        && points.iter().all(|p| ((p[0] + 10.0).hypot(p[1] - 10.0) - 1.6).abs() < 1e-9))
    });
    assert!(ring);
  }

  #[test]
  fn names() {
    assert_eq!(WeldKind::parse("V groove"), Some(WeldKind::V));
    assert_eq!(WeldKind::parse("I"), Some(WeldKind::Square));
    assert_eq!(WeldKind::parse("fillet"), Some(WeldKind::Fillet));
    assert_eq!(WeldKind::parse("rivet"), None);
    for k in WeldKind::ALL {
      assert_eq!(WeldKind::parse(k.key()), Some(k));
      assert_eq!(serde_json::to_value(k).unwrap(), k.key());
    }
    assert_eq!(WeldSide::parse("other"), Some(WeldSide::Other));
  }

  #[test]
  fn composite_frames() {
    let upper = FrameTier { tolerance: 0.5, diameter: true, modifier: None, datums: vec!["A".into(), "B".into()] };
    let lower = FrameTier { tolerance: 0.1, diameter: true, modifier: None, datums: vec!["A".into()] };
    assert!(lower.check_composite(Characteristic::Position, &upper).is_ok());
    assert!(lower.check_composite(Characteristic::Flatness, &upper).is_err());
    let loose = FrameTier { tolerance: 0.6, ..lower.clone() };
    assert!(loose.check_composite(Characteristic::Position, &upper).is_err());
    let wrong = FrameTier { datums: vec!["B".into()], ..lower.clone() };
    assert!(wrong.check_composite(Characteristic::Position, &upper).is_err());

    let mut plot = Plot::default();
    composite_lower(&mut plot, Standard::Asme, [0.0, 0.0], [10.0, 10.0], &upper, &lower);
    assert_eq!(plot.texts(), ["Ø0.1", "A"]);
    // the lower tier sits a cell under the upper one, from the frame's left edge
    let pts: Vec<[f64; 2]> = plot
      .items
      .iter()
      .filter_map(|i| if let Item::Line { points, .. } = i { Some(points.clone()) } else { None })
      .flatten()
      .collect();
    let (lo_x, lo_y) = pts.iter().fold((f64::INFINITY, f64::INFINITY), |(x, y), p| (x.min(p[0]), y.min(p[1])));
    assert!((lo_x - 10.0).abs() < 1e-9 && (lo_y - (10.0 - 1.5 * CELL)).abs() < 1e-9, "{lo_x} {lo_y}");
  }

  #[test]
  fn datum_targets() {
    let mut point = Plot::default();
    datum_target(&mut point, Standard::Asme, [0.0, 0.0], [12.0, 12.0], "A1", 0.0, 1.0);
    assert_eq!(point.texts(), ["A1"]);
    // circle, split line, leader and the × (two strokes)
    assert_eq!(point.items.iter().filter(|i| matches!(i, Item::Line { .. })).count(), 5);
    let mut area = Plot::default();
    datum_target(&mut area, Standard::Iso, [0.0, 0.0], [12.0, 12.0], "B2", 6.5, 2.0);
    assert_eq!(area.texts(), ["B2", "Ø6,5"]);
    // the area's outline at the drawing's scale: 6.5 model mm at 2:1
    let ring = area.items.iter().find_map(|i| match i {
      Item::Line { pen: Pen::Center, points } => Some(points.clone()),
      _ => None,
    });
    assert!(ring.unwrap().iter().all(|p| (p[0].hypot(p[1]) - 6.5).abs() < 1e-9));
  }
}
