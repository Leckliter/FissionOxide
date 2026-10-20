//! Editable sketch text. A text keeps its string, font, height, angle and an anchor point (the
//! start of its baseline, an ordinary sketch point you can drag, dimension or constrain) and owns
//! the glyph outlines it generates: fixed lines and splines that form closed profiles like any
//! other geometry. Moving the anchor moves the glyphs after each solve; changing the string,
//! font or size regenerates them. Exploding a text leaves plain curves (what older sketches have).

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::{CurveId, CurveKind, PointId, Profile, PtSpec, Seg, Sketch, SketchError};

/// Most characters one text may have.
const MAX_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TextId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SketchText {
  pub id: TextId,
  pub text: String,
  /// One of [`text_fonts`], a font file's path, or the name of a font in the system's font
  /// folders (see [`crate::system_fonts`]).
  pub font: String,
  /// Capital letter height (mm).
  pub height: f64,
  /// Baseline direction (radians, counter-clockwise from the sketch X axis).
  pub angle: f64,
  /// The start of the first line's baseline.
  pub anchor: PointId,
  /// Glyph curves this text generated.
  pub curves: Vec<CurveId>,
  /// Glyph points (fixed; placed from `local` around the anchor).
  pub points: Vec<PointId>,
  /// Each glyph point in text coordinates (baseline along +X, before turning by `angle`).
  pub local: Vec<[f64; 2]>,
  /// Bold, italic, underline, alignment and character spacing (left out of files when plain).
  #[serde(default, skip_serializing_if = "TextStyle::is_plain")]
  pub style: TextStyle,
  /// The curve the text runs along instead of a straight baseline (`anchor` and `angle` are then
  /// unused).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub path: Option<TextPath>,
  /// For each glyph point, the middle of its glyph's advance along the baseline (text
  /// coordinates): along a path the glyph sits there, turned to the path's direction.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub pivots: Vec<f64>,
  /// Width of the longest line (mm), for aligning text along a path.
  #[serde(default)]
  pub width: f64,
}

/// Where a text's lines sit across the anchor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HAlign {
  /// Lines start at the anchor.
  #[default]
  Left,
  Center,
  /// Lines end at the anchor.
  Right,
}

/// Where a text's block of lines sits up and down from the anchor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VAlign {
  /// The first line's baseline is on the anchor.
  #[default]
  Baseline,
  /// The first line's capitals reach the anchor.
  Top,
  Middle,
  /// The last line's baseline is on the anchor.
  Bottom,
}

/// Fusion's text options beyond font and size.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
  /// Strokes thickened (outlines pushed out by a twentieth of the height).
  pub bold: bool,
  /// Slanted 12 degrees.
  pub italic: bool,
  /// A bar under each line (its own closed profile).
  pub underline: bool,
  pub halign: HAlign,
  pub valign: VAlign,
  /// Extra space between characters, in percent of the text height (negative packs them).
  pub spacing: f64,
}

impl TextStyle {
  pub fn is_plain(&self) -> bool {
    *self == TextStyle::default()
  }
}

/// Where along its path a text sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
  /// Starting at the path's start.
  #[default]
  Left,
  /// Centred on the path's middle.
  Center,
  /// Ending at the path's end.
  Right,
}

/// Text laid along a sketch curve, as Fusion's "along path" text. The text reads left to right
/// in the sketch: it runs from whichever end of the curve lies further to -X (lower Y when the
/// ends are level; a whole circle runs clockwise from angle 0), glyph tops on the left of that
/// direction, so above a quarter arc's top it stands on the outside.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextPath {
  pub curve: CurveId,
  /// Glyphs stand on the curve (true) or hang below it, their capitals reaching up to it (false).
  pub above: bool,
  #[serde(default)]
  pub align: TextAlign,
}

/// The built-in fonts: the UI's proportional font and its monospace font. A text can also use a
/// font file from disk, by path or by name (see [`crate::system_fonts`]).
pub fn text_fonts() -> &'static [&'static str] {
  &["sans", "mono"]
}

/// Glyph outlines in text coordinates: `contours` of points, with each curve as a list of indices.
#[derive(Default)]
struct Glyphs {
  points: Vec<[f64; 2]>,
  /// (is a line, point indices): lines have two, quadratic splines three, cubic ones four.
  curves: Vec<(bool, Vec<usize>)>,
  /// Per point: the middle of its glyph's advance (x).
  pivots: Vec<f64>,
  /// Width of the longest line.
  width: f64,
}

struct Outline<'a> {
  g: &'a mut Glyphs,
  map: &'a dyn Fn(f32, f32) -> [f64; 2],
  start: Option<usize>,
  last: Option<usize>,
}

impl Outline<'_> {
  fn add(&mut self, p: [f64; 2]) -> usize {
    self.g.points.push(p);
    self.g.points.len() - 1
  }

  /// A contour point; the closing point reuses the contour's first one.
  fn on(&mut self, x: f32, y: f32) -> usize {
    let q = (self.map)(x, y);
    if let Some(s) = self.start
      && near(self.g.points[s], q)
    {
      return s;
    }
    self.add(q)
  }
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
  (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9
}

impl ttf_parser::OutlineBuilder for Outline<'_> {
  fn move_to(&mut self, x: f32, y: f32) {
    let q = (self.map)(x, y);
    let i = self.add(q);
    self.start = Some(i);
    self.last = Some(i);
  }

  fn line_to(&mut self, x: f32, y: f32) {
    let Some(a) = self.last else { return };
    let b = self.on(x, y);
    if a != b && !near(self.g.points[a], self.g.points[b]) {
      self.g.curves.push((true, vec![a, b]));
    }
    self.last = Some(b);
  }

  fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
    let Some(a) = self.last else { return };
    let c = self.add((self.map)(x1, y1));
    let b = self.on(x, y);
    self.g.curves.push((false, vec![a, c, b]));
    self.last = Some(b);
  }

  fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
    let Some(a) = self.last else { return };
    let c1 = self.add((self.map)(x1, y1));
    let c2 = self.add((self.map)(x2, y2));
    let b = self.on(x, y);
    self.g.curves.push((false, vec![a, c1, c2, b]));
    self.last = Some(b);
  }

  fn close(&mut self) {
    if let (Some(a), Some(s)) = (self.last, self.start)
      && a != s
      && !near(self.g.points[a], self.g.points[s])
    {
      self.g.curves.push((true, vec![a, s]));
    }
    self.start = None;
    self.last = None;
  }
}

/// Outlines of `text` with capital letters `height` tall, styled by `style`; with the default
/// alignment the first baseline starts at (0, 0).
fn outline(text: &str, font: &str, height: f64, style: &TextStyle) -> Result<Glyphs, SketchError> {
  if !(height.is_finite() && height > 0.0) {
    return Err(SketchError::InvalidValue("the text height must be positive".into()));
  }
  if !style.spacing.is_finite() || style.spacing.abs() > 1000.0 {
    return Err(SketchError::InvalidValue(format!("character spacing {}%", style.spacing)));
  }
  if text.chars().count() > MAX_CHARS {
    return Err(SketchError::InvalidValue(format!("at most {MAX_CHARS} characters")));
  }
  let data = crate::fonts::font_data(font)?;
  let face = ttf_parser::Face::parse(data.bytes(), 0)
    .map_err(|e| SketchError::InvalidValue(format!("the font could not be read: {e}")))?;
  let k = height / cap_height(&face);
  let line_gap = f64::from(face.height()) * k;
  let bold = if style.bold { height * BOLD } else { 0.0 };
  let extra = style.spacing / 100.0 * height + 2.0 * bold;
  let slant = if style.italic { ITALIC.to_radians().tan() } else { 0.0 };
  let mut g = Glyphs::default();
  let mut pen = [0.0f64, 0.0f64];
  // each line: its first point and curve, and its width
  let mut lines: Vec<(usize, usize, f64)> = Vec::new();
  let mut line_start = (0, 0);
  let mut width = 0.0f64;
  for ch in text.chars() {
    if ch == '\n' {
      lines.push((line_start.0, line_start.1, width));
      line_start = (g.points.len(), g.curves.len());
      width = 0.0;
      pen = [0.0, pen[1] - line_gap];
      continue;
    }
    let Some(id) = face.glyph_index(ch).or_else(|| face.glyph_index('?')) else { continue };
    let origin = pen;
    let map = move |x: f32, y: f32| -> [f64; 2] {
      let (x, y) = (f64::from(x) * k, f64::from(y) * k);
      [origin[0] + bold + x + y * slant, origin[1] + y]
    };
    let (p0, c0) = (g.points.len(), g.curves.len());
    let mut o = Outline { g: &mut g, map: &map, start: None, last: None };
    face.outline_glyph(id, &mut o);
    if bold > 0.0 {
      embolden(&mut g, p0, c0, bold);
    }
    let advance = f64::from(face.glyph_hor_advance(id).unwrap_or(0)) * k;
    g.pivots.resize(g.points.len(), pen[0] + bold + advance / 2.0);
    width = pen[0] + advance + 2.0 * bold;
    g.width = g.width.max(width);
    pen[0] += advance + extra;
  }
  lines.push((line_start.0, line_start.1, width));
  if style.underline {
    // a bar under each line, as long as the line, after the glyphs it belongs to
    let mut with_bars = Glyphs::default();
    let mut moved = Vec::new();
    for (i, &(p0, c0, w)) in lines.iter().enumerate() {
      let (p1, c1) = lines.get(i + 1).map_or((g.points.len(), g.curves.len()), |l| (l.0, l.1));
      let base = with_bars.points.len();
      with_bars.points.extend_from_slice(&g.points[p0..p1]);
      with_bars.pivots.extend_from_slice(&g.pivots[p0..p1]);
      for (line, idx) in &g.curves[c0..c1] {
        with_bars.curves.push((*line, idx.iter().map(|j| j - p0 + base).collect()));
      }
      let start = with_bars.points.len();
      if w > 0.0 {
        let y = -(i as f64) * line_gap - UNDERLINE.0 * height;
        let t = UNDERLINE.1 * height;
        with_bars.points.extend_from_slice(&[[0.0, y], [w, y], [w, y - t], [0.0, y - t]]);
        with_bars.pivots.resize(with_bars.points.len(), w / 2.0);
        for j in 0..4 {
          with_bars.curves.push((true, vec![start + j, start + (j + 1) % 4]));
        }
      }
      moved.push((base, with_bars.curves.len() - (c1 - c0) - if w > 0.0 { 4 } else { 0 }, w));
    }
    with_bars.width = g.width;
    g = with_bars;
    lines = moved;
  }
  // alignment: each line across the anchor, the block up and down from it
  let n = lines.len();
  for (i, &(p0, _, w)) in lines.iter().enumerate() {
    let p1 = lines.get(i + 1).map_or(g.points.len(), |l| l.0);
    let dx = match style.halign {
      HAlign::Left => 0.0,
      HAlign::Center => -w / 2.0,
      HAlign::Right => -w,
    };
    let bottom = -((n - 1) as f64) * line_gap;
    let dy = match style.valign {
      VAlign::Baseline => 0.0,
      VAlign::Top => -height,
      VAlign::Middle => -(height + bottom) / 2.0,
      VAlign::Bottom => -bottom,
    };
    for p in &mut g.points[p0..p1] {
      p[0] += dx;
      p[1] += dy;
    }
    for m in &mut g.pivots[p0..p1] {
      *m += dx;
    }
  }
  Ok(g)
}

/// How far bold pushes outlines out, as a fraction of the text height.
const BOLD: f64 = 0.05;
/// Italic slant (degrees).
const ITALIC: f64 = 12.0;
/// Underline: its gap below the baseline and its thickness, as fractions of the text height.
const UNDERLINE: (f64, f64) = (0.15, 0.07);

/// Thickens the glyph whose points start at `p0` and curves at `c0`: every point moves out from
/// its contour by `d` (along the corner's bisector, as FreeType's emboldening does), so outer
/// outlines grow and holes shrink.
fn embolden(g: &mut Glyphs, p0: usize, c0: usize, d: f64) {
  let n = g.points.len() - p0;
  let (mut prev, mut next) = (vec![None; n], vec![None; n]);
  let mut area = 0.0;
  for (_, idx) in &g.curves[c0..] {
    for w in idx.windows(2) {
      let (a, b) = (w[0] - p0, w[1] - p0);
      next[a] = Some(b);
      prev[b] = Some(a);
      let (pa, pb) = (g.points[w[0]], g.points[w[1]]);
      area += pa[0] * pb[1] - pb[0] * pa[1];
    }
  }
  // outward is to the right of a counter-clockwise outline's direction and left of a clockwise one
  let side = if area >= 0.0 { 1.0 } else { -1.0 };
  let normal = |a: [f64; 2], b: [f64; 2]| -> Option<[f64; 2]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy);
    (l > 1e-12).then(|| [side * dy / l, -side * dx / l])
  };
  let pts = g.points[p0..].to_vec();
  for i in 0..n {
    let (Some(a), Some(b)) = (prev[i], next[i]) else { continue };
    let (n1, n2) = (normal(pts[a], pts[i]), normal(pts[i], pts[b]));
    let m = match (n1, n2) {
      (Some(u), Some(v)) => {
        let s = [u[0] + v[0], u[1] + v[1]];
        let dot = 1.0 + u[0] * v[0] + u[1] * v[1];
        // the bisector, long enough that both sides move by d (capped at sharp corners)
        if dot > 0.2 { [s[0] / dot, s[1] / dot] } else { u }
      }
      (Some(u), None) | (None, Some(u)) => u,
      (None, None) => continue,
    };
    g.points[p0 + i] = [pts[i][0] + m[0] * d, pts[i][1] + m[1] * d];
  }
}

/// Font units that `height` means. Fusion puts the top of a capital A 1.00076 heights up
/// (measured in Arial, Calibri, Courier New and Times New Roman, whose A stands taller than its
/// H), so the scale comes from the A glyph, then H; fonts with neither use the OS/2 capital
/// height.
fn cap_height(face: &ttf_parser::Face) -> f64 {
  const TOP_OVER_HEIGHT: f64 = 1.00076;
  let top = |ch: char| {
    face.glyph_index(ch).and_then(|id| face.glyph_bounding_box(id)).map(|b| f64::from(b.y_max)).filter(|h| *h > 0.0)
  };
  match top('A').or_else(|| top('H')) {
    Some(h) => h / TOP_OVER_HEIGHT,
    None => face.capital_height().filter(|h| *h > 0).map(f64::from).unwrap_or(f64::from(face.ascender()) * 0.7),
  }
}

impl Sketch {
  pub fn text(&self, id: TextId) -> Option<&SketchText> {
    self.texts.iter().find(|t| t.id == id)
  }

  /// The text a glyph curve belongs to.
  pub fn text_of(&self, curve: CurveId) -> Option<TextId> {
    self.texts.iter().find(|t| t.curves.contains(&curve)).map(|t| t.id)
  }

  /// Adds editable text whose first baseline starts at `at`, capitals `height` tall, turned by
  /// `angle` (radians).
  pub fn add_text(
    &mut self,
    at: PtSpec,
    text: &str,
    font: &str,
    height: f64,
    angle: f64,
  ) -> Result<TextId, SketchError> {
    self.add_text_styled(at, text, font, height, angle, TextStyle::default())
  }

  /// [`Sketch::add_text`] with bold, italic, underline, alignment and spacing.
  pub fn add_text_styled(
    &mut self,
    at: PtSpec,
    text: &str,
    font: &str,
    height: f64,
    angle: f64,
    style: TextStyle,
  ) -> Result<TextId, SketchError> {
    if !angle.is_finite() {
      return Err(SketchError::InvalidValue(format!("text angle {angle}")));
    }
    // check everything before adding anything
    outline(text, font, height, &style)?;
    let anchor = self.resolve(at)?;
    self.push_text(anchor, text, font, height, angle, style, None)
  }

  /// Adds editable text laid along a curve (see [`TextPath`]), capitals `height` tall.
  pub fn add_text_on_path(
    &mut self,
    path: TextPath,
    text: &str,
    font: &str,
    height: f64,
  ) -> Result<TextId, SketchError> {
    outline(text, font, height, &TextStyle::default())?;
    let anchor = self.path_anchor(path.curve)?;
    self.push_text(anchor, text, font, height, 0.0, TextStyle::default(), Some(path))
  }

  /// Lays a text along a curve, or (None) back on its straight baseline at its anchor.
  pub fn set_text_path(&mut self, id: TextId, path: Option<TextPath>) -> Result<(), SketchError> {
    self.text(id).ok_or(SketchError::InvalidValue(format!("no text {}", id.0)))?;
    let anchor = match path {
      Some(p) => Some(self.path_anchor(p.curve)?),
      None => None,
    };
    if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
      t.path = path;
      if let Some(a) = anchor {
        t.anchor = a;
      }
    }
    self.place_texts();
    Ok(())
  }

  /// The point a text along `curve` is anchored to (one of the curve's own, so deleting the text
  /// leaves it): the curve must be a plain curve of this sketch, not a glyph.
  fn path_anchor(&self, curve: CurveId) -> Result<PointId, SketchError> {
    self.curve(curve).ok_or(SketchError::NoCurve(curve))?;
    if self.text_of(curve).is_some() {
      return Err(SketchError::WrongGeometry("text along a path", "a plain curve (not another text)"));
    }
    if self.curve_segs(curve).is_empty() {
      return Err(SketchError::InvalidValue("the path curve has no length".into()));
    }
    self.curve_points(curve).first().copied().ok_or(SketchError::NoCurve(curve))
  }

  fn push_text(
    &mut self,
    anchor: PointId,
    text: &str,
    font: &str,
    height: f64,
    angle: f64,
    style: TextStyle,
    path: Option<TextPath>,
  ) -> Result<TextId, SketchError> {
    let id = TextId(self.alloc());
    let font = if font.is_empty() { "sans" } else { font };
    self.texts.push(SketchText {
      id,
      text: text.to_string(),
      font: font.to_string(),
      height,
      angle,
      anchor,
      curves: Vec::new(),
      points: Vec::new(),
      local: Vec::new(),
      style,
      path,
      pivots: Vec::new(),
      width: 0.0,
    });
    self.regenerate_text(id)?;
    Ok(id)
  }

  /// Changes a text's string, font, height or angle (None keeps it) and regenerates its glyphs.
  pub fn edit_text(
    &mut self,
    id: TextId,
    text: Option<&str>,
    font: Option<&str>,
    height: Option<f64>,
    angle: Option<f64>,
  ) -> Result<(), SketchError> {
    let t = self.text(id).ok_or(SketchError::InvalidValue(format!("no text {}", id.0)))?;
    let (text, font, height, angle) = (
      text.map(str::to_string).unwrap_or_else(|| t.text.clone()),
      font.map(str::to_string).unwrap_or_else(|| t.font.clone()),
      height.unwrap_or(t.height),
      angle.unwrap_or(t.angle),
    );
    if !angle.is_finite() {
      return Err(SketchError::InvalidValue(format!("text angle {angle}")));
    }
    outline(&text, &font, height, &t.style)?;
    if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
      t.text = text;
      t.font = font;
      t.height = height;
      t.angle = angle;
    }
    self.regenerate_text(id)
  }

  /// Changes a text's bold, italic, underline, alignment and spacing, and regenerates its glyphs.
  pub fn set_text_style(&mut self, id: TextId, style: TextStyle) -> Result<(), SketchError> {
    let t = self.text(id).ok_or(SketchError::InvalidValue(format!("no text {}", id.0)))?;
    outline(&t.text, &t.font, t.height, &style)?;
    if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
      t.style = style;
    }
    self.regenerate_text(id)
  }

  /// Turns a text into plain curves (free points), as text was before it became editable.
  pub fn explode_text(&mut self, id: TextId) -> Result<Vec<CurveId>, SketchError> {
    let i = self.texts.iter().position(|t| t.id == id).ok_or(SketchError::InvalidValue(format!("no text {}", id.0)))?;
    let t = self.texts.remove(i);
    for p in &mut self.points {
      if t.points.contains(&p.id) {
        p.fixed = false;
      }
    }
    Ok(t.curves)
  }

  /// Deletes a text: its glyphs, its anchor (unless other geometry uses it) and their constraints.
  pub fn remove_text(&mut self, id: TextId) -> Result<(), SketchError> {
    let i = self.texts.iter().position(|t| t.id == id).ok_or(SketchError::InvalidValue(format!("no text {}", id.0)))?;
    let t = self.texts.remove(i);
    self.clear_glyphs(&t);
    let used = self.curves.iter().any(|c| crate::curve_points(&c.kind).contains(&t.anchor))
      || self.texts.iter().any(|o| o.anchor == t.anchor);
    if t.anchor != crate::ORIGIN && !used {
      self.points.retain(|p| p.id != t.anchor);
      self.constraints.retain(|c| !c.kind.points().contains(&t.anchor));
    }
    Ok(())
  }

  fn clear_glyphs(&mut self, t: &SketchText) {
    self.curves.retain(|c| !t.curves.contains(&c.id));
    self.points.retain(|p| !t.points.contains(&p.id));
    self.constraints.retain(|c| {
      !c.kind.curves().iter().any(|x| t.curves.contains(x)) && !c.kind.points().iter().any(|x| t.points.contains(x))
    });
  }

  /// Rebuilds a text's glyph curves from its string, font and size.
  fn regenerate_text(&mut self, id: TextId) -> Result<(), SketchError> {
    let Some(t) = self.text(id).cloned() else { return Ok(()) };
    self.clear_glyphs(&t);
    let g = outline(&t.text, &t.font, t.height, &t.style)?;
    let ids: Vec<PointId> = g
      .points
      .iter()
      .map(|_| {
        let id = PointId(self.alloc());
        self.points.push(crate::Point { id, x: 0.0, y: 0.0, fixed: true });
        id
      })
      .collect();
    let mut curves = Vec::with_capacity(g.curves.len());
    for (line, idx) in &g.curves {
      let pts: Vec<PointId> = idx.iter().map(|i| ids[*i]).collect();
      let kind = match (line, pts.as_slice()) {
        (true, [a, b]) => CurveKind::Line { a: *a, b: *b },
        _ => CurveKind::control_spline(pts),
      };
      curves.push(self.add_curve(kind)?);
    }
    if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
      t.curves = curves;
      t.points = ids;
      t.local = g.points;
      t.pivots = g.pivots;
      t.width = g.width;
    }
    self.place_texts();
    Ok(())
  }

  /// Puts every text's glyph points where its anchor and angle (or its path) say.
  pub(crate) fn place_texts(&mut self) {
    if self.texts.is_empty() {
      return;
    }
    let index = self.point_index();
    let mut moves = Vec::new();
    for t in &self.texts {
      if let Some(path) = &t.path {
        self.place_on_path(t, path, &index, &mut moves);
        continue;
      }
      let Some(a) = self.pos(t.anchor) else { continue };
      let (s, c) = t.angle.sin_cos();
      for (p, l) in t.points.iter().zip(&t.local) {
        if let Some(&i) = index.get(p) {
          moves.push((i, [a[0] + l[0] * c - l[1] * s, a[1] + l[0] * s + l[1] * c]));
        }
      }
    }
    for (i, q) in moves {
      if let Some(p) = self.points.get_mut(i) {
        p.x = q[0];
        p.y = q[1];
      }
    }
  }

  /// Moves, turns and scales the texts that have glyphs among `curves` (as Move/Scale did their
  /// curves); `moved` are the points already moved.
  pub(crate) fn transform_texts(
    &mut self,
    curves: &[CurveId],
    t: &crate::Xform2,
    moved: &[PointId],
  ) -> Result<(), SketchError> {
    let hit: Vec<TextId> =
      self.texts.iter().filter(|x| x.curves.iter().any(|c| curves.contains(c))).map(|x| x.id).collect();
    for id in hit {
      let Some((anchor, height, angle, on_path)) =
        self.text(id).map(|x| (x.anchor, x.height, x.angle, x.path.is_some()))
      else {
        continue;
      };
      // text along a path follows its path; only its size changes here
      if !on_path
        && !moved.contains(&anchor)
        && let Some(p) = self.point_mut(anchor).filter(|p| !p.fixed)
      {
        [p.x, p.y] = t.apply([p.x, p.y]);
      }
      if let Some(x) = self.texts.iter_mut().find(|x| x.id == id) {
        x.height = height * t.scale;
        if !on_path {
          x.angle = angle + t.angle;
        }
      }
      self.regenerate_text(id)?;
    }
    Ok(())
  }

  /// Queues the glyph points of `t`, laid along `path`, as (point index, position) moves.
  fn place_on_path(
    &self,
    t: &SketchText,
    path: &TextPath,
    index: &HashMap<PointId, usize>,
    moves: &mut Vec<(usize, [f64; 2])>,
  ) {
    let Some(walk) = Walk::new(self.curve_segs(path.curve)) else { return };
    let start = match path.align {
      TextAlign::Left => 0.0,
      TextAlign::Center => (walk.total - t.width) / 2.0,
      TextAlign::Right => walk.total - t.width,
    };
    let drop = if path.above { 0.0 } else { t.height };
    for ((p, l), m) in t.points.iter().zip(&t.local).zip(&t.pivots) {
      let Some(&i) = index.get(p) else { continue };
      let (q, d) = walk.at(start + m);
      let (u, v) = (l[0] - m, l[1] - drop);
      moves.push((i, [q[0] + u * d[0] - v * d[1], q[1] + u * d[1] + v * d[0]]));
    }
  }

  /// Whether a region is a glyph's counter (the inside of an O, A or e): bounded by one text's
  /// glyph curves and lying inside an even number of them. A text's profile, as in Fusion, is its
  /// ink only, so features given no regions leave these out.
  pub fn is_text_counter(&self, region: &Profile) -> bool {
    let at = region.interior_point();
    self.texts.iter().any(|t| {
      let pts: Vec<[f64; 2]> = t.points.iter().filter_map(|p| self.pos(*p)).collect();
      let on_glyph = |q: [f64; 2]| pts.iter().any(|g| (g[0] - q[0]).hypot(g[1] - q[1]) < 1e-6);
      if !region.outer.segs.iter().all(|s| on_glyph(s.start())) {
        return false;
      }
      let mut crossings = 0usize;
      for c in &t.curves {
        for seg in self.curve_segs(*c) {
          let mut line = Vec::new();
          seg.flatten(&mut line);
          line.push(seg.end());
          for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a[1] > at[1]) != (b[1] > at[1]) && at[0] < a[0] + (at[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
              crossings += 1;
            }
          }
        }
      }
      crossings.is_multiple_of(2)
    })
  }

  /// Errors for modify tools that would edit a text's glyphs.
  pub(crate) fn not_text(&self, curve: CurveId) -> Result<(), SketchError> {
    match self.text_of(curve) {
      Some(_) => Err(SketchError::WrongGeometry("this tool", "plain curves (explode the text first)")),
      None => Ok(()),
    }
  }
}

/// A curve walked by length, in the direction text along it reads (see [`TextPath`]).
struct Walk {
  /// Pieces as polylines with their running lengths, or exact lines and arcs.
  pieces: Vec<(Seg, f64)>,
  total: f64,
  reversed: bool,
}

/// Samples per spline or ellipse piece when measuring its length.
const WALK_STEPS: usize = 64;

impl Walk {
  fn new(segs: Vec<Seg>) -> Option<Walk> {
    let (first, last) = (segs.first()?.start(), segs.last()?.end());
    let mut pieces = Vec::new();
    for s in segs {
      match s {
        Seg::Line { .. } | Seg::Arc { .. } | Seg::Circle { .. } => pieces.push((s, seg_length(&s))),
        _ => {
          // splines and ellipses as short chords
          let p = sample(&s);
          for w in p.windows(2) {
            let l = Seg::Line { a: w[0], b: w[1] };
            pieces.push((l, seg_length(&l)));
          }
        }
      }
    }
    pieces.retain(|(_, l)| *l > 1e-12);
    let total: f64 = pieces.iter().map(|(_, l)| l).sum();
    if total <= 1e-9 {
      return None;
    }
    let (dx, dy) = (last[0] - first[0], last[1] - first[1]);
    let closed = dx.hypot(dy) < 1e-9;
    let reversed = closed || dx < -1e-9 || (dx.abs() <= 1e-9 && dy < 0.0);
    Some(Walk { pieces, total, reversed })
  }

  /// The point `d` along the walk and the unit direction there; beyond the ends the walk carries
  /// straight on.
  fn at(&self, d: f64) -> ([f64; 2], [f64; 2]) {
    let raw = if self.reversed { self.total - d } else { d };
    let (p, t) = self.raw_at(raw);
    if self.reversed { (p, [-t[0], -t[1]]) } else { (p, t) }
  }

  fn raw_at(&self, d: f64) -> ([f64; 2], [f64; 2]) {
    let mut run = 0.0;
    for (k, (s, l)) in self.pieces.iter().enumerate() {
      if d <= run + l || k + 1 == self.pieces.len() {
        return seg_at(s, *l, d - run);
      }
      run += l;
    }
    ([0.0, 0.0], [1.0, 0.0])
  }
}

fn seg_length(s: &Seg) -> f64 {
  match *s {
    Seg::Line { a, b } => (b[0] - a[0]).hypot(b[1] - a[1]),
    Seg::Arc { radius, sweep, .. } => radius * sweep.abs(),
    Seg::Circle { radius, .. } => radius * std::f64::consts::TAU,
    _ => sample(s).windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum(),
  }
}

/// Points along a spline or ellipse piece, ends included.
fn sample(s: &Seg) -> Vec<[f64; 2]> {
  match *s {
    Seg::Bezier { p } => (0..=WALK_STEPS).map(|i| crate::bezier_at(&p, i as f64 / WALK_STEPS as f64)).collect(),
    _ => {
      let mut out = Vec::new();
      s.flatten(&mut out);
      out.push(s.end());
      out
    }
  }
}

/// The point `d` along a line, arc or circle of length `len` (extended past its ends), and the
/// unit direction there.
fn seg_at(s: &Seg, len: f64, d: f64) -> ([f64; 2], [f64; 2]) {
  let on_circle = |c: [f64; 2], r: f64, a0: f64, sign: f64| {
    let d = d.clamp(0.0, len);
    let a = a0 + sign * d / r;
    let (sn, cs) = a.sin_cos();
    ([c[0] + r * cs, c[1] + r * sn], [-sn * sign, cs * sign])
  };
  let (p, t) = match *s {
    Seg::Line { a, b } => {
      let t = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
      return ([a[0] + t[0] * d, a[1] + t[1] * d], t);
    }
    Seg::Arc { center, radius, start_angle, sweep } => on_circle(center, radius, start_angle, sweep.signum()),
    Seg::Circle { center, radius, ccw } => on_circle(center, radius, 0.0, if ccw { 1.0 } else { -1.0 }),
    _ => (s.start(), [1.0, 0.0]),
  };
  // carry straight on past an arc's ends
  let over = if d < 0.0 { d } else { (d - len).max(0.0) };
  ([p[0] + t[0] * over, p[1] + t[1] * over], t)
}

#[cfg(test)]
mod tests {
  use crate::*;

  #[test]
  fn text_regenerates_and_follows_its_anchor() {
    let mut s = Sketch::new();
    let t = s.add_text(PtSpec::At(10.0, 5.0), "HI", "sans", 10.0, 0.0).unwrap();
    let n = s.curves.len();
    assert!(n >= 2, "two glyphs");
    assert_eq!(find_profiles(&s).len(), 2, "H and I are closed outlines");
    let (min, max) = s.bounds().unwrap();
    assert!((min[1] - 5.0).abs() < 0.2 && (max[1] - 15.0).abs() < 0.3, "{min:?} {max:?}");
    // dragging the anchor moves the glyphs with it
    let anchor = s.text(t).unwrap().anchor;
    assert!(s.drag(anchor, [30.0, 5.0]).ok());
    let (min2, _) = s.bounds().unwrap();
    assert!((min2[0] - min[0] - 20.0).abs() < 1e-6, "{min:?} {min2:?}");
    // editing regenerates
    s.edit_text(t, Some("HII"), None, Some(20.0), None).unwrap();
    assert!(s.curves.len() > n);
    let (_, max3) = s.bounds().unwrap();
    assert!((max3[1] - 25.0).abs() < 0.5, "{max3:?}");
    assert_eq!(find_profiles(&s).len(), 3);
    // a text survives a save, and deleting a glyph deletes the text
    let back: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
    let c = s.text(t).unwrap().curves[0];
    s.remove_curve(c).unwrap();
    assert!(s.texts.is_empty() && s.curves.is_empty());
    assert_eq!(s.points.len(), 1, "only the origin is left");
  }

  #[test]
  fn moving_and_scaling_glyphs_moves_and_scales_the_text() {
    let mut s = Sketch::new();
    let t = s.add_text(PtSpec::At(10.0, 0.0), "T", "sans", 5.0, 0.0).unwrap();
    let curves = s.text(t).unwrap().curves.clone();
    let xf = Xform2 { scale: 2.0, angle: 0.0, center: [0.0, 0.0], translate: [5.0, 0.0] };
    s.transform(&curves, &xf).unwrap();
    assert!(s.solve().ok());
    let txt = s.text(t).unwrap();
    assert!((txt.height - 10.0).abs() < 1e-12);
    assert_eq!(s.pos(txt.anchor), Some([25.0, 0.0]));
    let (min, max) = s.bounds().unwrap();
    assert!(min[0] >= 25.0 - 1e-6 && (max[1] - 10.0).abs() < 0.3, "{min:?} {max:?}");
  }

  #[test]
  fn exploded_text_is_plain_curves_and_old_sketches_load() {
    let mut s = Sketch::new();
    let t = s.add_text(PtSpec::At(0.0, 0.0), "O", "mono", 5.0, 0.3).unwrap();
    assert!(s.trim(s.text(t).unwrap().curves[0], [0.0, 0.0]).is_err(), "glyphs are not trimmed");
    let curves = s.explode_text(t).unwrap();
    assert!(s.texts.is_empty() && !curves.is_empty());
    assert!(s.points.iter().filter(|p| p.id != ORIGIN).all(|p| !p.fixed));
    // a sketch saved before editable text has no "texts"
    let mut json: serde_json::Value = serde_json::to_value(&s).unwrap();
    json.as_object_mut().unwrap().remove("texts");
    let back: Sketch = serde_json::from_value(json).unwrap();
    assert_eq!(back.curves.len(), curves.len());
    assert!(s.add_text(PtSpec::At(0.0, 0.0), "x", "comic-no-such-font", 5.0, 0.0).is_err());
    assert!(s.add_text(PtSpec::At(0.0, 0.0), "x", "sans", 0.0, 0.0).is_err());
  }

  #[test]
  fn text_styles_change_the_glyphs() {
    let plain = |style: TextStyle| {
      let mut s = Sketch::new();
      let t = s.add_text_styled(PtSpec::At(0.0, 0.0), "HI", "sans", 10.0, 0.0, style).unwrap();
      (s, t)
    };
    let (base, _) = plain(TextStyle::default());
    let (lo, hi) = base.bounds().unwrap();
    let width = hi[0] - lo[0];
    // right-aligned text ends at the anchor, centred text straddles it
    let (s, _) = plain(TextStyle { halign: HAlign::Right, ..Default::default() });
    let (_, hr) = s.bounds().unwrap();
    assert!(hr[0] <= 0.0 + 1e-9 && hr[0] > -2.0, "{hr:?}");
    let (s, _) = plain(TextStyle { halign: HAlign::Center, valign: VAlign::Top, ..Default::default() });
    let (l, h) = s.bounds().unwrap();
    assert!(l[0] < 0.0 && h[0] > 0.0 && (h[1]).abs() < 0.3, "{l:?} {h:?}");
    let (s, _) = plain(TextStyle { valign: VAlign::Middle, ..Default::default() });
    let (l, h) = s.bounds().unwrap();
    assert!((l[1] + h[1]).abs() < 0.3, "{l:?} {h:?}");
    // spacing and bold widen it, italic leans it right at the top
    let (s, _) = plain(TextStyle { spacing: 50.0, ..Default::default() });
    let (l, h) = s.bounds().unwrap();
    assert!((h[0] - l[0]) - width > 4.0, "{width} {l:?} {h:?}");
    let (s, _) = plain(TextStyle { bold: true, ..Default::default() });
    let (l, h) = s.bounds().unwrap();
    assert!((h[0] - l[0]) > width + 0.5 && h[1] > hi[1] + 0.3, "{l:?} {h:?}");
    assert_eq!(find_profiles(&s).len(), 2, "bold glyphs stay closed profiles");
    let (s, _) = plain(TextStyle { italic: true, ..Default::default() });
    let (_, h) = s.bounds().unwrap();
    assert!(h[0] > hi[0] + 1.0, "{h:?} {hi:?}");
    // underline: a bar below the baseline, its own profile
    let (s, t) = plain(TextStyle { underline: true, ..Default::default() });
    let (l, _) = s.bounds().unwrap();
    assert!(l[1] < -1.0, "{l:?}");
    assert_eq!(find_profiles(&s).len(), 3);
    // plain text files keep their old form; styled ones round-trip
    assert!(!serde_json::to_string(&base).unwrap().contains("style"));
    let back: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
    // restyling regenerates
    let mut s = s;
    s.set_text_style(t, TextStyle::default()).unwrap();
    assert_eq!(find_profiles(&s).len(), 2);
  }

  #[test]
  fn text_along_an_arc_stands_on_it_from_its_left_end() {
    let mut s = Sketch::new();
    // a quarter arc from (20, 0) counter-clockwise to (0, 20): text reads from its left end
    let arc = s.add_arc_center(PtSpec::At(0.0, 0.0), PtSpec::At(20.0, 0.0), [0.0, 20.0]).unwrap();
    let path = TextPath { curve: arc, above: true, align: TextAlign::Left };
    let t = s.add_text_on_path(path, "LL", "sans", 5.0).unwrap();
    let radii = |s: &Sketch| -> Vec<f64> {
      s.text(t).unwrap().points.iter().map(|p| s.pos(*p).unwrap()).map(|q| q[0].hypot(q[1])).collect()
    };
    let r = radii(&s);
    assert!(r.iter().all(|r| *r > 20.0 - 1e-6 && *r < 26.0), "{r:?}");
    let pts: Vec<[f64; 2]> = s.text(t).unwrap().points.iter().map(|p| s.pos(*p).unwrap()).collect();
    let (min_x, max_y) = pts.iter().fold((f64::MAX, f64::MIN), |(x, y), q| (x.min(q[0]), y.max(q[1])));
    assert!(min_x > 0.0 && min_x < 1.0 && max_y > 24.0, "it starts at the arc's top end: {min_x} {max_y}");
    // hanging below the path keeps the glyphs inside it
    s.set_text_path(t, Some(TextPath { above: false, ..path })).unwrap();
    assert!(radii(&s).iter().all(|r| *r < 20.0 + 0.1), "{:?}", radii(&s));
    // centred: the glyphs move along to the middle of the arc
    s.set_text_path(t, Some(TextPath { align: TextAlign::Center, ..path })).unwrap();
    let pts: Vec<[f64; 2]> = s.text(t).unwrap().points.iter().map(|p| s.pos(*p).unwrap()).collect();
    let mid = pts.iter().fold([0.0, 0.0], |a, q| [a[0] + q[0], a[1] + q[1]]);
    assert!((mid[0] - mid[1]).abs() / (pts.len() as f64) < 1.0, "centred about 45 degrees: {mid:?}");
    // the profiles are the glyphs; deleting the path deletes the text, not the other way round
    assert_eq!(find_profiles(&s).len(), 2);
    let back: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
    s.remove_curve(arc).unwrap();
    assert!(s.texts.is_empty() && s.curves.is_empty());
  }

  #[test]
  fn glyph_counters_are_known() {
    let mut s = Sketch::new();
    s.add_text(PtSpec::At(0.0, 0.0), "IO", "sans", 10.0, 0.0).unwrap();
    s.add_circle(PtSpec::At(50.0, 0.0), 3.0).unwrap();
    let profiles = find_profiles(&s);
    let counters: Vec<bool> = profiles.iter().map(|p| s.is_text_counter(p)).collect();
    assert_eq!(profiles.len(), 4, "I, O ring, O counter and the circle");
    assert_eq!(counters.iter().filter(|c| **c).count(), 1, "{counters:?}");
  }
}
