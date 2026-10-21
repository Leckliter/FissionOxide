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
//!
//! Supplementary symbols sit on the weld symbol's far face: a contour (flush, convex or concave;
//! parallel to the face of a fillet) with the finishing method's letter beyond it (AWS: C chip,
//! G grind, H hammer, M machine, R roll, U unspecified). A back (backing run) weld is an open
//! half circle and a melt-through a filled one, on the side of the reference line opposite the
//! groove weld they go with.
//!
//! A datum target is a point (an ×), a line (between two points: a phantom line in ASME
//! Y14.5, a thin line between two × in ISO 5459) or a circular area.

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

  /// A groove weld (one a back weld or melt-through can go with).
  pub fn groove(self) -> bool {
    matches!(self, WeldKind::Square | WeldKind::V | WeldKind::Bevel | WeldKind::U | WeldKind::J)
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
  /// Contour of the weld face: the weld's side's (the arrow side's for a both-sides weld).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub contour: Option<WeldContour>,
  /// The finishing method's letter beyond the contour.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub finish: String,
  /// The other side's contour and finish of a both-sides weld.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub other_contour: Option<WeldContour>,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub other_finish: String,
  /// A back weld or melt-through opposite a groove weld.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub back: Option<BackWeld>,
}

impl WeldSymbol {
  /// A plain symbol: no extras.
  pub fn new(kind: WeldKind, side: WeldSide) -> WeldSymbol {
    WeldSymbol {
      kind,
      side,
      size: String::new(),
      length: String::new(),
      all_around: false,
      field: false,
      tail: String::new(),
      contour: None,
      finish: String::new(),
      other_contour: None,
      other_finish: String::new(),
      back: None,
    }
  }

  /// Do the supplementary symbols go together (AWS A2.4 / ISO 2553)?
  pub fn check(&self) -> Result<(), String> {
    for (contour, finish) in [(self.contour, &self.finish), (self.other_contour, &self.other_finish)] {
      if finish.is_empty() {
        continue;
      }
      if !FINISHES.iter().any(|(k, _)| k == finish) {
        let all: Vec<String> = FINISHES.iter().map(|(k, n)| format!("{k} {n}")).collect();
        return Err(format!("unknown finish '{finish}' ({})", all.join(", ")));
      }
      if contour.is_none() {
        return Err("a finish letter goes with a contour (flush, convex or concave)".into());
      }
    }
    if self.side != WeldSide::Both && (self.other_contour.is_some() || !self.other_finish.is_empty()) {
      return Err("only a both-sides weld has an other-side contour".into());
    }
    if let Some(b) = self.back {
      if !self.kind.groove() {
        return Err(format!("a {} goes opposite a groove weld (square, V, bevel, U or J)", b.name()));
      }
      if self.side == WeldSide::Both {
        return Err(format!("a {} goes on the side opposite a one-sided groove weld", b.name()));
      }
    }
    Ok(())
  }
}

/// The finishing methods' letters (AWS A2.4).
pub const FINISHES: [(&str, &str); 6] =
  [("C", "chip"), ("G", "grind"), ("H", "hammer"), ("M", "machine"), ("R", "roll"), ("U", "unspecified")];

/// The contour of a weld's face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldContour {
  Flush,
  Convex,
  Concave,
}

impl WeldContour {
  pub fn parse(s: &str) -> Option<WeldContour> {
    Some(match s.trim().to_ascii_lowercase().as_str() {
      "flush" | "flat" => WeldContour::Flush,
      "convex" => WeldContour::Convex,
      "concave" => WeldContour::Concave,
      _ => return None,
    })
  }
}

/// A weld on the back of a groove weld.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackWeld {
  /// A back or backing weld (ISO: backing run): an open half circle.
  Back,
  /// Complete joint penetration showing on the back: a filled half circle.
  MeltThrough,
}

impl BackWeld {
  pub fn parse(s: &str) -> Option<BackWeld> {
    Some(match s.trim().to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
      "back" | "backing" | "back_weld" | "backing_run" => BackWeld::Back,
      "melt_through" | "melt" => BackWeld::MeltThrough,
      _ => return None,
    })
  }

  fn name(self) -> &'static str {
    match self {
      BackWeld::Back => "back weld",
      BackWeld::MeltThrough => "melt-through",
    }
  }
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

/// A point along a shallow arc from `a` to `b` bulging `sag` along `n` (t in 0..1).
fn bulge(a: [f64; 2], b: [f64; 2], n: [f64; 2], sag: f64, t: f64) -> [f64; 2] {
  let k = sag * 4.0 * t * (1.0 - t);
  [a[0] + (b[0] - a[0]) * t + n[0] * k, a[1] + (b[1] - a[1]) * t + n[1] * k]
}

/// The contour on a weld symbol's far face (parallel to a fillet's face), with the finish letter
/// beyond it; `base` and `s` as in [`weld_shape`].
fn face_contour(plot: &mut Plot, kind: WeldKind, c: WeldContour, finish: &str, x: f64, base: f64, s: f64) {
  // the contour's ends and its outward normal, in (along, away from the line) symbol units
  let (a, b, n) = if kind == WeldKind::Fillet {
    let l = SYM.hypot(3.0);
    let n = [SYM / l, 3.0 / l];
    let off = |p: [f64; 2], k: f64| [p[0] + n[0] * k, p[1] + n[1] * k];
    let (top, toe) = ([-1.5, SYM], [1.5, 0.0]);
    let (a, b) = (off(top, 0.9), off(toe, 0.9));
    // a little shorter than the face
    let shrink = |p: [f64; 2], q: [f64; 2]| [p[0] + (q[0] - p[0]) * 0.12, p[1] + (q[1] - p[1]) * 0.12];
    (shrink(a, b), shrink(b, a), n)
  } else {
    ([-1.8, SYM + 1.0], [1.8, SYM + 1.0], [0.0, 1.0])
  };
  let sag = match c {
    WeldContour::Flush => 0.0,
    WeldContour::Convex => 0.7,
    WeldContour::Concave => -0.7,
  };
  let to = |q: [f64; 2]| [x + q[0], base + s * q[1]];
  line(plot, (0..=12).map(|i| to(bulge(a, b, n, sag, i as f64 / 12.0))).collect());
  if !finish.is_empty() {
    // the letter beyond the contour's middle
    let m = bulge(a, b, n, sag.max(0.0), 0.5);
    let far = m[1] + n[1] * 0.8;
    let at = [x + m[0] + n[0] * 1.6, if s > 0.0 { base + far } else { base - far - SMALL }];
    plot.items.push(Item::Text { at, size: SMALL, text: finish.into(), anchor: Anchor::Middle });
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
  // (base of the symbol, which way it points, contour, finish) for each side the weld is on
  let iso = std == Standard::Iso;
  let (arrow_side, other_side) = if iso { ((y, 1.0), (y - DASH_GAP, -1.0)) } else { ((y, -1.0), (y, 1.0)) };
  let mut places = Vec::new();
  let own = (w.contour, w.finish.as_str());
  match w.side {
    WeldSide::Arrow => places.push((arrow_side, own)),
    WeldSide::Other => places.push((other_side, own)),
    // a symmetric weld straddles the plain reference line, the arrow side where it'd be alone
    WeldSide::Both => {
      let (up, down) = if iso {
        (own, (w.other_contour, w.other_finish.as_str()))
      } else {
        ((w.other_contour, w.other_finish.as_str()), own)
      };
      places.extend([((y, 1.0), up), ((y, -1.0), down)]);
    }
  }
  if iso && w.side != WeldSide::Both {
    let yd = y - DASH_GAP;
    plot.items.push(Item::Line { pen: Pen::Hidden, points: vec![[x0, yd], [x1, yd]] });
  }
  for ((base, s), (contour, finish)) in places {
    weld_shape(plot, w.kind, cx, base, s);
    if let Some(c) = contour {
      face_contour(plot, w.kind, c, finish, cx, base, s);
    }
    let mid = base + s * SYM / 2.0 - SMALL / 2.0;
    if !w.size.is_empty() {
      plot.items.push(Item::Text { at: [cx - 3.0, mid], size: SMALL, text: w.size.clone(), anchor: Anchor::End });
    }
    if !w.length.is_empty() {
      plot.items.push(Item::Text { at: [cx + 3.0, mid], size: SMALL, text: w.length.clone(), anchor: Anchor::Start });
    }
  }
  if let Some(b) = w.back {
    // across the reference line from the groove weld
    let (base, s) = if w.side == WeldSide::Other { arrow_side } else { other_side };
    let half: Vec<[f64; 2]> =
      arc([0.0, 0.0], 1.3, 0.0, std::f64::consts::PI, 12).iter().map(|q| [cx + q[0], base + s * q[1]]).collect();
    match b {
      BackWeld::Back => line(plot, half),
      BackWeld::MeltThrough => plot.items.push(Item::Fill { points: half }),
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
/// ("A1") below, on a leader to an × (a point target), to the area's outline or to the middle of
/// a target line from `anchor` to `end` (sheet mm); `area` is the area's diameter in model mm
/// (0 for a point or a line), `k` sheet mm per model mm.
#[allow(clippy::too_many_arguments)]
pub(super) fn datum_target(
  plot: &mut Plot,
  std: Standard,
  anchor: [f64; 2],
  end: Option<[f64; 2]>,
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
  let cross = |plot: &mut Plot, p: [f64; 2]| {
    let x = 1.5;
    line(plot, vec![[p[0] - x, p[1] - x], [p[0] + x, p[1] + x]]);
    line(plot, vec![[p[0] - x, p[1] + x], [p[0] + x, p[1] - x]]);
  };
  // a line seen end on is a point
  let end = end.filter(|e| (e[0] - anchor[0]).hypot(e[1] - anchor[1]) > 1e-6);
  let to = match end {
    Some(e) => {
      if std == Standard::Iso {
        // ISO 5459: a thin line between two crosses
        line(plot, vec![anchor, e]);
        cross(plot, anchor);
        cross(plot, e);
      } else {
        // ASME Y14.5: a phantom line
        plot.items.push(Item::Line { pen: Pen::Phantom, points: vec![anchor, e] });
      }
      [(anchor[0] + e[0]) / 2.0, (anchor[1] + e[1]) / 2.0]
    }
    None => anchor,
  };
  let d = [to[0] - c[0], to[1] - c[1]];
  let l = d[0].hypot(d[1]);
  if l > r {
    line(plot, vec![[c[0] + d[0] / l * r, c[1] + d[1] / l * r], to]);
  }
  if area <= 0.0 && end.is_none() {
    cross(plot, anchor);
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
    WeldSymbol { size: "6".into(), length: "50-150".into(), ..WeldSymbol::new(kind, side) }
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
    datum_target(&mut point, Standard::Asme, [0.0, 0.0], None, [12.0, 12.0], "A1", 0.0, 1.0);
    assert_eq!(point.texts(), ["A1"]);
    // circle, split line, leader and the × (two strokes)
    assert_eq!(point.items.iter().filter(|i| matches!(i, Item::Line { .. })).count(), 5);
    let mut area = Plot::default();
    datum_target(&mut area, Standard::Iso, [0.0, 0.0], None, [12.0, 12.0], "B2", 6.5, 2.0);
    assert_eq!(area.texts(), ["B2", "Ø6,5"]);
    // the area's outline at the drawing's scale: 6.5 model mm at 2:1
    let ring = area.items.iter().find_map(|i| match i {
      Item::Line { pen: Pen::Center, points } => Some(points.clone()),
      _ => None,
    });
    assert!(ring.unwrap().iter().all(|p| (p[0].hypot(p[1]) - 6.5).abs() < 1e-9));
  }

  #[test]
  fn datum_target_lines() {
    // ASME: a phantom line, the leader to its middle, no ×
    let mut asme = Plot::default();
    datum_target(&mut asme, Standard::Asme, [0.0, 0.0], Some([20.0, 0.0]), [0.0, 15.0], "A1", 0.0, 1.0);
    let phantom: Vec<_> = asme
      .items
      .iter()
      .filter_map(|i| if let Item::Line { pen: Pen::Phantom, points } = i { Some(points.clone()) } else { None })
      .collect();
    assert_eq!(phantom, [vec![[0.0, 0.0], [20.0, 0.0]]]);
    // circle, split line and the leader, which ends on the line's middle
    let thin: Vec<_> = asme
      .items
      .iter()
      .filter_map(|i| if let Item::Line { pen: Pen::Thin, points } = i { Some(points.clone()) } else { None })
      .collect();
    assert_eq!(thin.len(), 3);
    assert_eq!(thin[2].last(), Some(&[10.0, 0.0]));
    // ISO 5459: a thin line between two crosses
    let mut iso = Plot::default();
    datum_target(&mut iso, Standard::Iso, [0.0, 0.0], Some([20.0, 0.0]), [0.0, 15.0], "A1", 0.0, 1.0);
    assert!(!iso.items.iter().any(|i| matches!(i, Item::Line { pen: Pen::Phantom, .. })));
    assert_eq!(iso.items.iter().filter(|i| matches!(i, Item::Line { pen: Pen::Thin, .. })).count(), 2 + 1 + 4 + 1);
    // a line seen end on is a point target
    let mut end_on = Plot::default();
    datum_target(&mut end_on, Standard::Asme, [0.0, 0.0], Some([0.0, 0.0]), [12.0, 12.0], "A1", 0.0, 1.0);
    assert_eq!(end_on.items.iter().filter(|i| matches!(i, Item::Line { .. })).count(), 5);
  }

  /// Thin strokes of a symbol drawn with and without `extra`, the reference line at y = 10.
  fn extra_strokes(std: Standard, base: &WeldSymbol, extra: &WeldSymbol) -> Vec<Vec<[f64; 2]>> {
    let strokes = |w: &WeldSymbol| {
      let mut plot = Plot::default();
      weld(&mut plot, std, [0.0, 0.0], [10.0, 10.0], w);
      plot.items
    };
    let before = strokes(base);
    strokes(extra)
      .into_iter()
      .filter(|i| !before.contains(i))
      .filter_map(|i| match i {
        Item::Line { points, .. } | Item::Fill { points } => Some(points),
        _ => None,
      })
      .collect()
  }

  fn y_range(strokes: &[Vec<[f64; 2]>]) -> (f64, f64) {
    strokes.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])))
  }

  #[test]
  fn contours_sit_beyond_the_symbol() {
    for kind in WeldKind::ALL {
      let w = sym(kind, WeldSide::Arrow);
      for c in [WeldContour::Flush, WeldContour::Convex, WeldContour::Concave] {
        // AWS arrow side: under the reference line, under the symbol
        let s = extra_strokes(Standard::Asme, &w, &WeldSymbol { contour: Some(c), ..w.clone() });
        assert_eq!(s.len(), 1, "{kind:?} {c:?}");
        let (lo, hi) = y_range(&s);
        assert!(hi < 10.0 - 0.5 && lo > 10.0 - SYM - 3.0, "{kind:?} {c:?} {lo} {hi}");
        // ISO arrow side: over the solid line
        let s = extra_strokes(Standard::Iso, &w, &WeldSymbol { contour: Some(c), ..w.clone() });
        let (lo, _) = y_range(&s);
        assert!(lo > 10.0 + 0.5, "{kind:?} {c:?}");
      }
    }
    // a flush contour is straight; convex bulges away from the weld, concave toward it
    let w = sym(WeldKind::V, WeldSide::Other);
    let mid = |c| {
      let s = extra_strokes(Standard::Asme, &w, &WeldSymbol { contour: Some(c), ..w.clone() });
      let p = &s[0];
      (p[p.len() / 2][1], p[0][1])
    };
    let (flush, end) = mid(WeldContour::Flush);
    assert!((flush - end).abs() < 1e-9);
    assert!(mid(WeldContour::Convex).0 > end + 0.5);
    assert!(mid(WeldContour::Concave).0 < end - 0.5);
    // a fillet's contour runs parallel to its face
    let f = sym(WeldKind::Fillet, WeldSide::Other);
    let s = extra_strokes(Standard::Asme, &f, &WeldSymbol { contour: Some(WeldContour::Flush), ..f.clone() });
    let (a, b) = (s[0][0], *s[0].last().unwrap());
    let slope = (b[1] - a[1]) / (b[0] - a[0]);
    assert!((slope + SYM / 3.0).abs() < 1e-9, "{slope}");
  }

  #[test]
  fn finish_letters_and_both_sides() {
    let mut w = sym(WeldKind::V, WeldSide::Both);
    w.contour = Some(WeldContour::Flush);
    w.finish = "G".into();
    w.other_contour = Some(WeldContour::Convex);
    w.other_finish = "M".into();
    assert!(w.check().is_ok());
    let y = |std: Standard, t: &str| {
      let mut plot = Plot::default();
      weld(&mut plot, std, [0.0, 0.0], [10.0, 10.0], &w);
      plot
        .items
        .iter()
        .find_map(|i| if let Item::Text { at, text, .. } = i { (text == t).then_some(at[1]) } else { None })
    };
    // the arrow side's finish below the line in AWS, above it in ISO; beyond the contour
    assert!(y(Standard::Asme, "G").unwrap() < 10.0 - SYM - 1.0);
    assert!(y(Standard::Asme, "M").unwrap() > 10.0 + SYM + 1.0);
    assert!(y(Standard::Iso, "G").unwrap() > 10.0 + SYM + 1.0);
    assert!(y(Standard::Iso, "M").unwrap() < 10.0 - SYM - 1.0);

    let bad = |f: &dyn Fn(&mut WeldSymbol)| {
      let mut x = sym(WeldKind::V, WeldSide::Arrow);
      f(&mut x);
      x.check().is_err()
    };
    assert!(bad(&|x| x.finish = "G".into()));
    assert!(bad(&|x| {
      x.contour = Some(WeldContour::Flush);
      x.finish = "Z".into();
    }));
    assert!(bad(&|x| x.other_contour = Some(WeldContour::Flush)));
    assert!(bad(&|x| {
      x.kind = WeldKind::Fillet;
      x.back = Some(BackWeld::Back);
    }));
    assert!(bad(&|x| {
      x.side = WeldSide::Both;
      x.back = Some(BackWeld::MeltThrough);
    }));
    assert!(!bad(&|x| x.back = Some(BackWeld::MeltThrough)));
    assert_eq!(WeldContour::parse("Flat"), Some(WeldContour::Flush));
    assert_eq!(BackWeld::parse("melt-through"), Some(BackWeld::MeltThrough));
    assert_eq!(BackWeld::parse("backing run"), Some(BackWeld::Back));
  }

  #[test]
  fn back_welds_go_opposite_the_groove() {
    for (std, side, above) in [
      // AWS: arrow side below, so the back weld is above
      (Standard::Asme, WeldSide::Arrow, true),
      (Standard::Asme, WeldSide::Other, false),
      // ISO: the arrow side on the solid line, the back weld under the dashed one
      (Standard::Iso, WeldSide::Arrow, false),
      (Standard::Iso, WeldSide::Other, true),
    ] {
      let w = sym(WeldKind::V, side);
      let back = extra_strokes(std, &w, &WeldSymbol { back: Some(BackWeld::Back), ..w.clone() });
      let (lo, hi) = y_range(&back);
      if above {
        assert!(lo >= 10.0 - 1e-9 && hi > 10.5, "{std:?} {side:?}");
      } else {
        assert!(hi <= 10.0 + 1e-9 && lo < 9.5, "{std:?} {side:?}");
      }
      // melt-through: the same half circle, filled
      let mut plot = Plot::default();
      weld(&mut plot, std, [0.0, 0.0], [10.0, 10.0], &WeldSymbol { back: Some(BackWeld::MeltThrough), ..w.clone() });
      let fills: Vec<_> =
        plot.items.iter().filter(|i| matches!(i, Item::Fill { points } if points.len() == 13)).collect();
      assert_eq!(fills.len(), 1);
    }
  }
}
