//! Editable sketch text. A text keeps its string, font, height, angle and an anchor point (the
//! start of its baseline, an ordinary sketch point you can drag, dimension or constrain) and owns
//! the glyph outlines it generates: fixed lines and splines that form closed profiles like any
//! other geometry. Moving the anchor moves the glyphs after each solve; changing the string,
//! font or size regenerates them. Exploding a text leaves plain curves (what older sketches have).

use serde::{Deserialize, Serialize};

use crate::{CurveId, CurveKind, PointId, PtSpec, Sketch, SketchError};

/// Most characters one text may have.
const MAX_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TextId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SketchText {
  pub id: TextId,
  pub text: String,
  /// One of [`text_fonts`].
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
}

/// The fonts text can use: the UI's proportional font and its monospace font.
pub fn text_fonts() -> &'static [&'static str] {
  &["sans", "mono"]
}

fn font_data(font: &str) -> Result<&'static [u8], SketchError> {
  match font {
    "" | "sans" => Ok(epaint_default_fonts::UBUNTU_LIGHT),
    "mono" => Ok(epaint_default_fonts::HACK_REGULAR),
    other => Err(SketchError::InvalidValue(format!("unknown font '{other}' (sans or mono)"))),
  }
}

/// Glyph outlines in text coordinates: `contours` of points, with each curve as a list of indices.
#[derive(Default)]
struct Glyphs {
  points: Vec<[f64; 2]>,
  /// (is a line, point indices): lines have two, quadratic splines three, cubic ones four.
  curves: Vec<(bool, Vec<usize>)>,
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

/// Outlines of `text` with capital letters `height` tall, the first baseline starting at (0, 0).
fn outline(text: &str, font: &str, height: f64) -> Result<Glyphs, SketchError> {
  if !(height.is_finite() && height > 0.0) {
    return Err(SketchError::InvalidValue("the text height must be positive".into()));
  }
  if text.chars().count() > MAX_CHARS {
    return Err(SketchError::InvalidValue(format!("at most {MAX_CHARS} characters")));
  }
  let face = ttf_parser::Face::parse(font_data(font)?, 0)
    .map_err(|e| SketchError::InvalidValue(format!("the font could not be read: {e}")))?;
  let cap = face.capital_height().filter(|h| *h > 0).map(f64::from).unwrap_or(f64::from(face.ascender()) * 0.7);
  let k = height / cap;
  let line_gap = f64::from(face.height()) * k;
  let mut g = Glyphs::default();
  let mut pen = [0.0f64, 0.0f64];
  for ch in text.chars() {
    if ch == '\n' {
      pen = [0.0, pen[1] - line_gap];
      continue;
    }
    let Some(id) = face.glyph_index(ch).or_else(|| face.glyph_index('?')) else { continue };
    let origin = pen;
    let map = move |x: f32, y: f32| -> [f64; 2] { [origin[0] + f64::from(x) * k, origin[1] + f64::from(y) * k] };
    let mut o = Outline { g: &mut g, map: &map, start: None, last: None };
    face.outline_glyph(id, &mut o);
    pen[0] += f64::from(face.glyph_hor_advance(id).unwrap_or(0)) * k;
  }
  Ok(g)
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
    if !angle.is_finite() {
      return Err(SketchError::InvalidValue(format!("text angle {angle}")));
    }
    // check everything before adding anything
    outline(text, font, height)?;
    let anchor = self.resolve(at)?;
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
    outline(&text, &font, height)?;
    if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
      t.text = text;
      t.font = font;
      t.height = height;
      t.angle = angle;
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
    let g = outline(&t.text, &t.font, t.height)?;
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
        _ => CurveKind::ControlSpline { points: pts },
      };
      curves.push(self.add_curve(kind)?);
    }
    if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
      t.curves = curves;
      t.points = ids;
      t.local = g.points;
    }
    self.place_texts();
    Ok(())
  }

  /// Puts every text's glyph points where its anchor and angle say.
  pub(crate) fn place_texts(&mut self) {
    if self.texts.is_empty() {
      return;
    }
    let index = self.point_index();
    let mut moves = Vec::new();
    for t in &self.texts {
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

  /// Errors for modify tools that would edit a text's glyphs.
  pub(crate) fn not_text(&self, curve: CurveId) -> Result<(), SketchError> {
    match self.text_of(curve) {
      Some(_) => Err(SketchError::WrongGeometry("this tool", "plain curves (explode the text first)")),
      None => Ok(()),
    }
  }
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
    assert!(s.add_text(PtSpec::At(0.0, 0.0), "x", "comic", 5.0, 0.0).is_err());
    assert!(s.add_text(PtSpec::At(0.0, 0.0), "x", "sans", 0.0, 0.0).is_err());
  }
}
