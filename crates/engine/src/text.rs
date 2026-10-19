//! Sketch text: glyph outlines (Ubuntu Light, the UI font) as sketch lines and splines, so text
//! makes closed profiles that extrude, cut or emboss like any other sketch.

use clf_sketch::{CurveKind, PointId, PtSpec, Sketch};

use crate::CmdError;

/// Most characters one text command may add.
const MAX_CHARS: usize = 500;

struct Outline<'a> {
  sketch: &'a mut Sketch,
  /// Font units → sketch coordinates.
  map: &'a dyn Fn(f32, f32) -> [f64; 2],
  start: Option<(PointId, [f64; 2])>,
  last: Option<(PointId, [f64; 2])>,
  error: Option<String>,
}

impl Outline<'_> {
  fn point(&mut self, x: f32, y: f32) -> (PointId, [f64; 2]) {
    let q = (self.map)(x, y);
    // the contour's closing point reuses its first one
    if let Some((id, s)) = self.start
      && (s[0] - q[0]).hypot(s[1] - q[1]) < 1e-9
    {
      return (id, s);
    }
    (self.sketch.add_point(q[0], q[1]), q)
  }

  fn add(&mut self, kind: CurveKind) {
    if self.error.is_none()
      && let Err(e) = self.sketch.add_curve(kind)
    {
      self.error = Some(e.to_string());
    }
  }
}

impl ttf_parser::OutlineBuilder for Outline<'_> {
  fn move_to(&mut self, x: f32, y: f32) {
    let q = (self.map)(x, y);
    let id = self.sketch.add_point(q[0], q[1]);
    self.start = Some((id, q));
    self.last = self.start;
  }

  fn line_to(&mut self, x: f32, y: f32) {
    let Some((a, pa)) = self.last else { return };
    let (b, pb) = self.point(x, y);
    if a != b
      && (pa[0] - pb[0]).hypot(pa[1] - pb[1]) > 1e-9
      && let Err(e) = self.sketch.add_line(PtSpec::Point(a), PtSpec::Point(b))
    {
      self.error.get_or_insert(e.to_string());
    }
    self.last = Some((b, pb));
  }

  fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
    let Some((a, _)) = self.last else { return };
    let c = (self.map)(x1, y1);
    let c = self.sketch.add_point(c[0], c[1]);
    let (b, pb) = self.point(x, y);
    self.add(CurveKind::ControlSpline { points: vec![a, c, b] });
    self.last = Some((b, pb));
  }

  fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
    let Some((a, _)) = self.last else { return };
    let (c1, c2) = ((self.map)(x1, y1), (self.map)(x2, y2));
    let (c1, c2) = (self.sketch.add_point(c1[0], c1[1]), self.sketch.add_point(c2[0], c2[1]));
    let (b, pb) = self.point(x, y);
    self.add(CurveKind::ControlSpline { points: vec![a, c1, c2, b] });
    self.last = Some((b, pb));
  }

  fn close(&mut self) {
    if let (Some((a, pa)), Some((s, ps))) = (self.last, self.start)
      && a != s
      && (pa[0] - ps[0]).hypot(pa[1] - ps[1]) > 1e-9
      && let Err(e) = self.sketch.add_line(PtSpec::Point(a), PtSpec::Point(s))
    {
      self.error.get_or_insert(e.to_string());
    }
    self.start = None;
    self.last = None;
  }
}

/// Adds `text` with its baseline starting at `at`, capital letters `height` tall, turned by
/// `angle` (radians). Returns the number of glyphs drawn.
pub fn add_text(sketch: &mut Sketch, at: [f64; 2], text: &str, height: f64, angle: f64) -> Result<usize, CmdError> {
  if !(height.is_finite() && height > 0.0) {
    return Err(CmdError::new("the text height must be positive"));
  }
  if text.chars().count() > MAX_CHARS {
    return Err(CmdError::new(format!("at most {MAX_CHARS} characters")));
  }
  let face = ttf_parser::Face::parse(epaint_default_fonts::UBUNTU_LIGHT, 0)
    .map_err(|e| CmdError::new(format!("the font could not be read: {e}")))?;
  let cap = face.capital_height().filter(|h| *h > 0).map(f64::from).unwrap_or(f64::from(face.ascender()) * 0.7);
  let k = height / cap;
  let line_gap = f64::from(face.height()) * k;
  let (c, s) = (angle.cos(), angle.sin());
  let mut pen = [0.0f64, 0.0f64];
  let mut drawn = 0;
  for ch in text.chars() {
    if ch == '\n' {
      pen = [0.0, pen[1] - line_gap];
      continue;
    }
    let Some(g) = face.glyph_index(ch).or_else(|| face.glyph_index('?')) else { continue };
    let origin = pen;
    let map = move |x: f32, y: f32| -> [f64; 2] {
      let (lx, ly) = (origin[0] + f64::from(x) * k, origin[1] + f64::from(y) * k);
      [at[0] + lx * c - ly * s, at[1] + lx * s + ly * c]
    };
    let mut o = Outline { sketch, map: &map, start: None, last: None, error: None };
    if face.outline_glyph(g, &mut o).is_some() {
      drawn += 1;
    }
    if let Some(e) = o.error {
      return Err(CmdError::new(format!("'{ch}': {e}")));
    }
    pen[0] += f64::from(face.glyph_hor_advance(g).unwrap_or(0)) * k;
  }
  Ok(drawn)
}
