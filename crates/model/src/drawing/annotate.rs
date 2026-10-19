//! Drawing annotations beyond plain dimensions: ordinate and angular dimensions, GD&T (feature
//! control frames, datum identifiers, surface finish symbols) and parts list balloons.
//!
//! Everything is drawn from pen strokes, fills and text, so it writes the same to SVG, PDF and
//! DXF. Symbols are vector strokes rather than font glyphs (Helvetica in a PDF has none of them).
//! Annotations hang off model points in a view, so they move with the view, with breaks and with
//! parameter changes.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use serde::{Deserialize, Serialize};

use super::{DimOrientation, Projected, TEXT, arrow, fmt_len, sheet_pt, upright};
use crate::plot::{Anchor, Item, Pen, Plot, text_width};

/// The drafting standard annotations follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Standard {
  /// ASME Y14.5-2018 / Y14.36: decimal point, surface texture value in the symbol's V.
  #[default]
  Asme,
  /// ISO 1101 / ISO 21920 (1302): decimal comma, "Ra" values on the symbol's extension line.
  Iso,
}

impl Standard {
  pub fn parse(s: &str) -> Option<Standard> {
    Some(match s.to_ascii_lowercase().as_str() {
      "asme" | "ansi" => Standard::Asme,
      "iso" | "din" => Standard::Iso,
      _ => return None,
    })
  }

  pub fn name(self) -> &'static str {
    match self {
      Standard::Asme => "ASME",
      Standard::Iso => "ISO",
    }
  }

  /// A value as the standard writes it: "0.05" (ASME) or "0,05" (ISO).
  pub fn number(self, v: f64) -> String {
    let s = fmt_len(v);
    if self == Standard::Iso { s.replace('.', ",") } else { s }
  }
}

/// Geometric characteristics (ASME Y14.5 / ISO 1101).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Characteristic {
  Straightness,
  Flatness,
  Circularity,
  Cylindricity,
  ProfileOfLine,
  ProfileOfSurface,
  Angularity,
  Perpendicularity,
  Parallelism,
  Position,
  Concentricity,
  Symmetry,
  CircularRunout,
  TotalRunout,
}

/// Whether a characteristic takes datum references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Datums {
  /// Form tolerances: never.
  None,
  Optional,
  Required,
}

impl Characteristic {
  pub const ALL: [Characteristic; 14] = [
    Characteristic::Straightness,
    Characteristic::Flatness,
    Characteristic::Circularity,
    Characteristic::Cylindricity,
    Characteristic::ProfileOfLine,
    Characteristic::ProfileOfSurface,
    Characteristic::Angularity,
    Characteristic::Perpendicularity,
    Characteristic::Parallelism,
    Characteristic::Position,
    Characteristic::Concentricity,
    Characteristic::Symmetry,
    Characteristic::CircularRunout,
    Characteristic::TotalRunout,
  ];

  pub fn parse(s: &str) -> Option<Characteristic> {
    let s = s.to_ascii_lowercase().replace([' ', '-'], "_");
    let alias = match s.as_str() {
      "roundness" => "circularity",
      "true_position" => "position",
      "coaxiality" => "concentricity",
      "profile_line" | "line_profile" => "profile_of_line",
      "profile_surface" | "surface_profile" | "profile" => "profile_of_surface",
      "runout" => "circular_runout",
      o => o,
    };
    Characteristic::ALL.into_iter().find(|c| c.key() == alias)
  }

  pub fn key(self) -> &'static str {
    match self {
      Characteristic::Straightness => "straightness",
      Characteristic::Flatness => "flatness",
      Characteristic::Circularity => "circularity",
      Characteristic::Cylindricity => "cylindricity",
      Characteristic::ProfileOfLine => "profile_of_line",
      Characteristic::ProfileOfSurface => "profile_of_surface",
      Characteristic::Angularity => "angularity",
      Characteristic::Perpendicularity => "perpendicularity",
      Characteristic::Parallelism => "parallelism",
      Characteristic::Position => "position",
      Characteristic::Concentricity => "concentricity",
      Characteristic::Symmetry => "symmetry",
      Characteristic::CircularRunout => "circular_runout",
      Characteristic::TotalRunout => "total_runout",
    }
  }

  pub fn datums(self) -> Datums {
    match self {
      Characteristic::Straightness
      | Characteristic::Flatness
      | Characteristic::Circularity
      | Characteristic::Cylindricity => Datums::None,
      Characteristic::ProfileOfLine | Characteristic::ProfileOfSurface | Characteristic::Position => Datums::Optional,
      _ => Datums::Required,
    }
  }

  /// Is it in the standard? ASME Y14.5-2018 dropped concentricity and symmetry.
  pub fn in_standard(self, s: Standard) -> bool {
    s == Standard::Iso || !matches!(self, Characteristic::Concentricity | Characteristic::Symmetry)
  }
}

/// Material condition modifier of a tolerance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialCondition {
  /// Ⓜ maximum material condition.
  Mmc,
  /// Ⓛ least material condition.
  Lmc,
}

impl MaterialCondition {
  fn letter(self) -> &'static str {
    match self {
      MaterialCondition::Mmc => "M",
      MaterialCondition::Lmc => "L",
    }
  }
}

/// How a surface may be made (the surface texture symbol's shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishProcess {
  /// Any process (open V).
  #[default]
  Any,
  /// Material removal required (V closed by a bar).
  Removal,
  /// Material removal prohibited (circle in the V).
  NoRemoval,
}

impl FinishProcess {
  pub fn parse(s: &str) -> Option<FinishProcess> {
    Some(match s.to_ascii_lowercase().as_str() {
      "any" => FinishProcess::Any,
      "removal" | "machined" | "required" => FinishProcess::Removal,
      "no_removal" | "prohibited" | "none" => FinishProcess::NoRemoval,
      _ => return None,
    })
  }
}

/// Frame cell height (sheet mm): twice the text height, as both standards draw it.
const CELL: f64 = 2.0 * TEXT;

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
  [a[0] + b[0], a[1] + b[1]]
}

fn unit(a: [f64; 2], b: [f64; 2]) -> Option<[f64; 2]> {
  let d = [b[0] - a[0], b[1] - a[1]];
  let l = d[0].hypot(d[1]);
  (l > 1e-9).then(|| [d[0] / l, d[1] / l])
}

fn line(plot: &mut Plot, points: Vec<[f64; 2]>) {
  plot.items.push(Item::Line { pen: Pen::Thin, points });
}

fn circle(c: [f64; 2], r: f64, n: usize) -> Vec<[f64; 2]> {
  (0..=n).map(|i| TAU * i as f64 / n as f64).map(|t| [c[0] + r * t.cos(), c[1] + r * t.sin()]).collect()
}

fn arc(c: [f64; 2], r: f64, from: f64, sweep: f64, n: usize) -> Vec<[f64; 2]> {
  (0..=n).map(|i| from + sweep * i as f64 / n as f64).map(|t| [c[0] + r * t.cos(), c[1] + r * t.sin()]).collect()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
  vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]]
}

/// Text centred on `c` (sheet mm).
fn centred(plot: &mut Plot, c: [f64; 2], size: f64, text: impl Into<String>) {
  plot.items.push(Item::Text { at: [c[0], c[1] - size / 2.0], size, text: text.into(), anchor: Anchor::Middle });
}

/// A leader from `anchor` (arrowhead) to `to`.
fn leader(plot: &mut Plot, anchor: [f64; 2], to: [f64; 2]) {
  if let Some(u) = unit(to, anchor) {
    line(plot, vec![anchor, to]);
    plot.items.push(arrow(anchor, u));
  }
}

/// Ordinate dimensions: each point's distance from `origin` along the view's horizontal (or
/// vertical), written at the end of an extension line `offset` mm past the farthest point.
#[allow(clippy::too_many_arguments)]
pub(super) fn ordinate(
  plot: &mut Plot,
  std: Standard,
  pv: &Projected,
  scale: f64,
  origin: [f64; 3],
  points: &[[f64; 3]],
  orientation: DimOrientation,
  offset: f64,
) {
  let i = if orientation == DimOrientation::Vertical { 1 } else { 0 };
  let j = 1 - i;
  let o = pv.to_view(origin.into());
  let marks: Vec<([f64; 2], f64)> = std::iter::once(&origin)
    .chain(points)
    .map(|p| {
      let v = pv.to_view((*p).into());
      (sheet_pt(pv, scale, v), v[i] - o[i])
    })
    .collect();
  let side = if offset < 0.0 { -1.0 } else { 1.0 };
  let far = marks.iter().map(|(p, _)| p[j] * side).fold(f64::NEG_INFINITY, f64::max) * side;
  let level = far + offset;
  for (p, value) in marks {
    let mut start = p;
    start[j] += side * 1.5;
    let mut end = p;
    end[j] = level;
    line(plot, vec![start, end]);
    let text = std.number(value);
    if i == 0 {
      // the value reads along the (vertical) extension line, past its end
      let w = text_width(&text, TEXT);
      let at = [p[0] + TEXT / 2.0, level + side * (w / 2.0 + 1.0)];
      plot.items.push(Item::RotatedText { at, size: TEXT, text, angle: 90.0 });
    } else {
      let anchor = if side > 0.0 { Anchor::Start } else { Anchor::End };
      plot.items.push(Item::Text { at: [level + side, p[1] - TEXT / 2.0], size: TEXT, text, anchor });
    }
  }
}

/// An angular dimension: an arc of `radius` sheet mm about `vertex` between the rays to `a` and
/// `b`, reading the true (model) angle.
#[allow(clippy::too_many_arguments)]
pub(super) fn angle(
  plot: &mut Plot,
  std: Standard,
  pv: &Projected,
  scale: f64,
  vertex: [f64; 3],
  a: [f64; 3],
  b: [f64; 3],
  radius: f64,
) {
  let pt = |p: [f64; 3]| sheet_pt(pv, scale, pv.to_view(p.into()));
  let (v, pa, pb) = (pt(vertex), pt(a), pt(b));
  let dir = |p: [f64; 2]| (p[1] - v[1]).atan2(p[0] - v[0]);
  let ta = dir(pa);
  let mut sweep = dir(pb) - ta;
  while sweep > PI {
    sweep -= TAU;
  }
  while sweep <= -PI {
    sweep += TAU;
  }
  let (va, vb) = (sub3(a, vertex), sub3(b, vertex));
  let (la, lb) = (norm(va), norm(vb));
  if sweep.abs() < 1e-9 || la < 1e-12 || lb < 1e-12 {
    return;
  }
  let r = radius.max(4.0);
  line(plot, arc(v, r, ta, sweep, 48));
  // extension lines when the measured edges stop short of the arc
  for (p, t) in [(pa, ta), (pb, ta + sweep)] {
    let l = (p[0] - v[0]).hypot(p[1] - v[1]);
    if l + 1.5 < r {
      let u = [t.cos(), t.sin()];
      line(plot, vec![add(v, [u[0] * (l + 1.5), u[1] * (l + 1.5)]), add(v, [u[0] * (r + 2.0), u[1] * (r + 2.0)])]);
    }
  }
  let s = sweep.signum();
  let tangent = |t: f64| [-t.sin() * s, t.cos() * s];
  let (t0, t1) = (tangent(ta), tangent(ta + sweep));
  plot.items.push(arrow([v[0] + r * ta.cos(), v[1] + r * ta.sin()], [-t0[0], -t0[1]]));
  plot.items.push(arrow([v[0] + r * (ta + sweep).cos(), v[1] + r * (ta + sweep).sin()], t1));
  let cos = ((va[0] * vb[0] + va[1] * vb[1] + va[2] * vb[2]) / (la * lb)).clamp(-1.0, 1.0);
  let text = format!("{}°", std.number(cos.acos().to_degrees()));
  let mid = ta + sweep / 2.0;
  let at = [v[0] + (r + 1.5) * mid.cos(), v[1] + (r + 1.5) * mid.sin()];
  plot.items.push(Item::RotatedText { at, size: TEXT, text, angle: upright((mid - FRAC_PI_2).to_degrees()) });
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn norm(v: [f64; 3]) -> f64 {
  (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// A geometric characteristic symbol centred on `c`, `s` mm from its centre to its edges.
fn symbol(plot: &mut Plot, ch: Characteristic, c: [f64; 2], s: f64) {
  let p = |x: f64, y: f64| [c[0] + x * s, c[1] + y * s];
  match ch {
    Characteristic::Straightness => line(plot, vec![p(-1.0, 0.0), p(1.0, 0.0)]),
    Characteristic::Flatness => {
      line(plot, vec![p(-1.0, -0.6), p(0.4, -0.6), p(1.0, 0.6), p(-0.4, 0.6), p(-1.0, -0.6)]);
    }
    Characteristic::Circularity => line(plot, circle(c, 0.9 * s, 32)),
    Characteristic::Cylindricity => {
      line(plot, circle(c, 0.65 * s, 32));
      line(plot, vec![p(-1.1, -1.0), p(-0.3, 1.0)]);
      line(plot, vec![p(0.3, -1.0), p(1.1, 1.0)]);
    }
    Characteristic::ProfileOfLine => line(plot, arc(p(0.0, -0.5), s, 0.0, PI, 24)),
    Characteristic::ProfileOfSurface => {
      let mut pts = arc(p(0.0, -0.5), s, 0.0, PI, 24);
      pts.push(p(1.0, -0.5));
      line(plot, pts);
    }
    Characteristic::Angularity => line(plot, vec![p(1.0, -0.7), p(-1.0, -0.7), p(0.75, 0.9)]),
    Characteristic::Perpendicularity => {
      line(plot, vec![p(-1.0, -0.9), p(1.0, -0.9)]);
      line(plot, vec![p(0.0, -0.9), p(0.0, 1.0)]);
    }
    Characteristic::Parallelism => {
      line(plot, vec![p(-1.0, -1.0), p(-0.2, 1.0)]);
      line(plot, vec![p(0.2, -1.0), p(1.0, 1.0)]);
    }
    Characteristic::Position => {
      line(plot, circle(c, 0.6 * s, 32));
      line(plot, vec![p(-1.0, 0.0), p(1.0, 0.0)]);
      line(plot, vec![p(0.0, -1.0), p(0.0, 1.0)]);
    }
    Characteristic::Concentricity => {
      line(plot, circle(c, 0.45 * s, 24));
      line(plot, circle(c, 0.95 * s, 32));
    }
    Characteristic::Symmetry => {
      line(plot, vec![p(-0.6, 0.6), p(0.6, 0.6)]);
      line(plot, vec![p(-1.0, 0.0), p(1.0, 0.0)]);
      line(plot, vec![p(-0.6, -0.6), p(0.6, -0.6)]);
    }
    Characteristic::CircularRunout => {
      line(plot, vec![p(-0.7, -1.0), p(0.7, 1.0)]);
      plot.items.push(small_head(p(0.7, 1.0), [0.7, 2.0]));
    }
    Characteristic::TotalRunout => {
      line(plot, vec![p(-1.0, -1.0), p(-0.1, 1.0)]);
      line(plot, vec![p(0.1, -1.0), p(1.0, 1.0)]);
      line(plot, vec![p(-1.0, -1.0), p(0.1, -1.0)]);
      plot.items.push(small_head(p(-0.1, 1.0), [0.9, 2.0]));
      plot.items.push(small_head(p(1.0, 1.0), [0.9, 2.0]));
    }
  }
}

/// A small filled arrowhead (runout symbols) pointing along `d`.
fn small_head(tip: [f64; 2], d: [f64; 2]) -> Item {
  let l = d[0].hypot(d[1]);
  let (u, n) = ([d[0] / l, d[1] / l], [-d[1] / l * 0.5, d[0] / l * 0.5]);
  let base = [tip[0] - u[0] * 1.6, tip[1] - u[1] * 1.6];
  Item::Fill { points: vec![tip, add(base, n), [base[0] - n[0], base[1] - n[1]]] }
}

/// A feature control frame `offset` mm from `anchor`, with a leader back to it.
#[allow(clippy::too_many_arguments)]
pub(super) fn feature_control(
  plot: &mut Plot,
  std: Standard,
  anchor: [f64; 2],
  offset: [f64; 2],
  ch: Characteristic,
  tolerance: f64,
  diameter: bool,
  modifier: Option<MaterialCondition>,
  datums: &[String],
) {
  let tol = format!("{}{}", if diameter { "Ø" } else { "" }, std.number(tolerance));
  let mod_w = if modifier.is_some() { 5.0 } else { 0.0 };
  let mut widths = vec![CELL, (text_width(&tol, TEXT) + 3.0 + mod_w).max(CELL)];
  widths.extend(datums.iter().map(|d| (text_width(d, TEXT) + 3.0).max(CELL)));
  let total: f64 = widths.iter().sum();
  let at = add(anchor, offset);
  // the frame grows away from the feature
  let x0 = if offset[0] >= 0.0 { at[0] } else { at[0] - total };
  let (y0, y1) = (at[1] - CELL / 2.0, at[1] + CELL / 2.0);
  line(plot, rect(x0, y0, x0 + total, y1));
  let mut x = x0;
  for w in &widths[..widths.len() - 1] {
    x += w;
    line(plot, vec![[x, y0], [x, y1]]);
  }
  symbol(plot, ch, [x0 + CELL / 2.0, at[1]], CELL * 0.3);
  let tol_x = x0 + CELL + 1.5;
  plot.items.push(Item::Text { at: [tol_x, y0 + (CELL - TEXT) / 2.0], size: TEXT, text: tol, anchor: Anchor::Start });
  if let Some(m) = modifier {
    let c = [x0 + CELL + widths[1] - 3.5, at[1]];
    line(plot, circle(c, 2.0, 24));
    centred(plot, c, 2.5, m.letter());
  }
  let mut x = x0 + CELL + widths[1];
  for (d, w) in datums.iter().zip(&widths[2..]) {
    centred(plot, [x + w / 2.0, at[1]], TEXT, d.clone());
    x += w;
  }
  let attach = [if offset[0] >= 0.0 { x0 } else { x0 + total }, at[1]];
  if (attach[0] - anchor[0]).hypot(attach[1] - anchor[1]) > 0.5 {
    leader(plot, anchor, attach);
  }
}

/// A datum feature identifier: the letter in a square frame, joined by a leader to a filled
/// triangle standing on the feature at `anchor`.
pub(super) fn datum(plot: &mut Plot, anchor: [f64; 2], offset: [f64; 2], label: &str) {
  let c = add(anchor, offset);
  let w = (text_width(label, TEXT) + 3.0).max(CELL);
  line(plot, rect(c[0] - w / 2.0, c[1] - CELL / 2.0, c[0] + w / 2.0, c[1] + CELL / 2.0));
  centred(plot, c, TEXT, label);
  // the frame side facing the feature
  let d = [anchor[0] - c[0], anchor[1] - c[1]];
  let attach = if d[0].abs() * CELL > d[1].abs() * w {
    [c[0] + w / 2.0 * d[0].signum(), c[1]]
  } else {
    [c[0], c[1] + CELL / 2.0 * d[1].signum()]
  };
  let u = unit(anchor, attach).unwrap_or([0.0, 1.0]);
  let n = [-u[1] * 1.5, u[0] * 1.5];
  let apex = [anchor[0] + u[0] * 2.6, anchor[1] + u[1] * 2.6];
  plot.items.push(Item::Fill { points: vec![add(anchor, n), [anchor[0] - n[0], anchor[1] - n[1]], apex] });
  if (attach[0] - apex[0]).hypot(attach[1] - apex[1]) > 0.1 {
    line(plot, vec![apex, attach]);
  }
}

/// A surface texture symbol with its tip `offset` mm from `anchor` (on a leader when apart).
pub(super) fn surface_finish(
  plot: &mut Plot,
  std: Standard,
  anchor: [f64; 2],
  offset: [f64; 2],
  roughness: f64,
  process: FinishProcess,
) {
  let tip = add(anchor, offset);
  if offset[0].hypot(offset[1]) > 0.5 {
    leader(plot, anchor, tip);
  }
  // legs at 60° to the surface: the short one 5 mm, the long one 10 mm (text height 3.5)
  let (c, s) = (0.5, 3f64.sqrt() / 2.0);
  let short = [tip[0] - 5.0 * c, tip[1] + 5.0 * s];
  let long = [tip[0] + 10.0 * c, tip[1] + 10.0 * s];
  line(plot, vec![short, tip, long]);
  match process {
    FinishProcess::Any => {}
    FinishProcess::Removal => line(plot, vec![short, [tip[0] + 5.0 * c, short[1]]]),
    FinishProcess::NoRemoval => line(plot, circle([tip[0], tip[1] + 5.0 / 3f64.sqrt()], 2.5 / 3f64.sqrt(), 24)),
  }
  let value = std.number(roughness);
  match std {
    Standard::Iso => {
      // ISO 21920-1: "Ra 1,6" over an extension of the long leg
      let text = format!("Ra {value}");
      let w = text_width(&text, TEXT) + 2.0;
      line(plot, vec![long, [long[0] + w, long[1]]]);
      plot.items.push(Item::Text { at: [long[0] + 1.0, long[1] + 1.0], size: TEXT, text, anchor: Anchor::Start });
    }
    Standard::Asme => {
      // ASME Y14.36: the Ra value (µm) above the short leg, left of the long one
      plot.items.push(Item::Text { at: [tip[0] + 2.2, short[1] + 0.8], size: TEXT, text: value, anchor: Anchor::End });
    }
  }
}

/// A balloon with a parts list item number, its leader ending in a dot on the part.
pub(super) fn balloon(plot: &mut Plot, anchor: [f64; 2], offset: [f64; 2], item: &str) {
  let c = add(anchor, offset);
  let r = (text_width(item, TEXT) / 2.0 + 1.5).max(4.0);
  line(plot, circle(c, r, 48));
  centred(plot, c, TEXT, item);
  if let Some(u) = unit(c, anchor)
    && offset[0].hypot(offset[1]) > r
  {
    line(plot, vec![anchor, [c[0] + u[0] * r, c[1] + u[1] * r]]);
    let dot = circle(anchor, 0.6, 8);
    plot.items.push(Item::Fill { points: dot[..8].to_vec() });
  }
}

/// Where an automatic balloon goes: just outside the view, out from its centre through the
/// part, turned until it clears the balloons already placed.
pub(super) fn auto_offset(pv: &Projected, scale: f64, anchor: [f64; 2], placed: &[[f64; 2]]) -> [f64; 2] {
  let at = pv.view.at.unwrap_or([0.0, 0.0]);
  let e = pv.extent(scale);
  let base = unit(at, anchor).map(|u| u[1].atan2(u[0])).unwrap_or(PI / 4.0);
  let mut best = None;
  for k in 0..24 {
    // 0, +15°, −15°, +30°, …
    let turn = ((k + 1) / 2) as f64 * if k % 2 == 1 { 1.0 } else { -1.0 } * PI / 12.0;
    let t = base + turn;
    let u = [t.cos(), t.sin()];
    let reach = (e[0] / 2.0 / u[0].abs().max(1e-9)).min(e[1] / 2.0 / u[1].abs().max(1e-9)) + 12.0;
    let c = [at[0] + u[0] * reach, at[1] + u[1] * reach];
    let clear = placed.iter().all(|q| (q[0] - c[0]).hypot(q[1] - c[1]) > 10.0);
    if best.is_none() || clear {
      best = Some(c);
    }
    if clear {
      break;
    }
  }
  let c = best.unwrap_or(at);
  [c[0] - anchor[0], c[1] - anchor[1]]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn names_and_numbers() {
    assert_eq!(Characteristic::parse("True Position"), Some(Characteristic::Position));
    assert_eq!(Characteristic::parse("total_runout"), Some(Characteristic::TotalRunout));
    assert_eq!(Characteristic::parse("roundness"), Some(Characteristic::Circularity));
    assert_eq!(Characteristic::parse("wobble"), None);
    for c in Characteristic::ALL {
      assert_eq!(Characteristic::parse(c.key()), Some(c));
    }
    assert!(!Characteristic::Symmetry.in_standard(Standard::Asme));
    assert!(Characteristic::Symmetry.in_standard(Standard::Iso));
    assert_eq!(Standard::Iso.number(0.05), "0,05");
    assert_eq!(Standard::Asme.number(0.05), "0.05");
  }

  #[test]
  fn every_symbol_draws() {
    for c in Characteristic::ALL {
      let mut plot = Plot::default();
      symbol(&mut plot, c, [10.0, 10.0], 2.0);
      assert!(!plot.items.is_empty(), "{c:?}");
      // symbols stay inside their cell
      for it in &plot.items {
        if let Item::Line { points, .. } | Item::Fill { points } = it {
          assert!(points.iter().all(|p| (p[0] - 10.0).abs() <= 2.3 && (p[1] - 10.0).abs() <= 2.3), "{c:?}");
        }
      }
    }
  }

  #[test]
  fn frame_cells() {
    let mut plot = Plot::default();
    let datums = vec!["A".to_string(), "B".to_string()];
    feature_control(
      &mut plot,
      Standard::Asme,
      [0.0, 0.0],
      [10.0, 10.0],
      Characteristic::Position,
      0.1,
      true,
      Some(MaterialCondition::Mmc),
      &datums,
    );
    let texts = plot.texts();
    assert_eq!(texts, vec!["Ø0.1", "M", "A", "B"]);
    // outline + 3 cell separators + 3 symbol strokes + modifier circle + leader, and the arrowhead
    let lines = plot.items.iter().filter(|i| matches!(i, Item::Line { .. })).count();
    assert_eq!(lines, 9);
    assert_eq!(plot.items.iter().filter(|i| matches!(i, Item::Fill { .. })).count(), 1);
  }

  #[test]
  fn surface_finish_follows_the_standard() {
    for (std, want) in [(Standard::Iso, "Ra 1,6"), (Standard::Asme, "1.6")] {
      let mut plot = Plot::default();
      surface_finish(&mut plot, std, [0.0, 0.0], [0.0, 0.0], 1.6, FinishProcess::Removal);
      assert_eq!(plot.texts(), vec![want]);
    }
  }
}
