//! Image tracing: a photo, logo or scan becomes closed contours of lines, arcs, circles and cubic
//! Béziers, ready to be written straight into a sketch.
//!
//! [`load`] decodes a PNG, JPEG, BMP or WebP file; [`trace`] then composites alpha on white,
//! downscales to [`Options::max_side`], optionally blurs, segments (a grey threshold or k-means
//! colours), drops specks and fills small holes, follows the pixel boundaries, splits them at
//! corners, fits each run within [`Options::tolerance`] and validates the result.
//!
//! Coordinates are pixels of the (downscaled) image: x right, **y up**, origin at the image's
//! bottom-left corner, so pixel `(x, y)` covers `[x, x+1] × [y, y+1]`.
//!
//! Guarantees, per layer:
//! - every contour is closed exactly: each segment starts bit-for-bit where the previous one
//!   ends, and the last ends where the first starts;
//! - no contour crosses or touches itself or another contour. Foreground is 8-connected: two
//!   pixels touching only at a corner belong to one shape, joined by a neck about 0.35 px wide;
//! - outer boundaries run counter-clockwise, holes clockwise (`hole: true`), and each outer
//!   boundary is followed directly by its own holes.
//!
//! [`Mode::Centerline`] instead thins the dark shapes to their centre lines and returns open
//! single-line [`Stroke`]s (for engraving); strokes meeting at a junction share its end point
//! exactly.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]
// `!(x > 0.0)` is deliberate: it rejects NaN as well as non-positive values.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

mod centerline;
mod clean;
mod contour;
mod fit;
mod raster;
mod segment;
mod svg;
mod validate;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_centerline;

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A point in pixels, y up.
pub(crate) type P = [f64; 2];

#[derive(Debug, thiserror::Error)]
pub enum TraceError {
  #[error("cannot read the image: {0}")]
  Io(#[from] std::io::Error),
  #[error("cannot decode the image: {0}")]
  Decode(#[from] image::ImageError),
  #[error("invalid image: {0}")]
  Image(String),
  #[error("invalid option: {0}")]
  Options(String),
}

/// An RGBA bitmap, 8 bits per channel, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
  pub width: u32,
  pub height: u32,
  /// `width * height * 4` bytes.
  pub rgba: Vec<u8>,
}

impl Image {
  /// An image from raw RGBA bytes (rows top to bottom); fails unless the length matches.
  pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Image, TraceError> {
    if width == 0 || height == 0 {
      return Err(TraceError::Image(format!("{width}×{height} has no pixels")));
    }
    let want = u64::from(width) * u64::from(height) * 4;
    if rgba.len() as u64 != want {
      return Err(TraceError::Image(format!("{width}×{height} needs {want} RGBA bytes, got {}", rgba.len())));
    }
    Ok(Image { width, height, rgba })
  }
}

/// Decodes a PNG, JPEG, BMP or WebP file (the format is sniffed from its content).
pub fn load(path: impl AsRef<Path>) -> Result<Image, TraceError> {
  let img = image::ImageReader::open(path)?.with_guessed_format()?.decode()?.into_rgba8();
  let (w, h) = img.dimensions();
  Image::from_rgba(w, h, img.into_raw())
}

/// Decodes an image held in memory.
pub fn decode(bytes: &[u8]) -> Result<Image, TraceError> {
  let img = image::load_from_memory(bytes)?.into_rgba8();
  let (w, h) = img.dimensions();
  Image::from_rgba(w, h, img.into_raw())
}

/// How the image is split into shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mode {
  /// One layer: pixels darker than `threshold` (grey level 0–255; `None` = Otsu's automatic
  /// level) are the shapes, or the lighter ones with [`Options::invert`].
  Outline {
    #[serde(default)]
    threshold: Option<u8>,
  },
  /// The image reduced to `count` colours (k-means in Lab, 1–64), one layer per colour.
  Colors { count: u8 },
  /// One layer of single-line strokes along the middle of the dark shapes (thresholded as in
  /// `Outline`), for engraving: [`Layer::strokes`] instead of closed contours.
  Centerline {
    #[serde(default)]
    threshold: Option<u8>,
  },
}

impl Default for Mode {
  fn default() -> Mode {
    Mode::Outline { threshold: None }
  }
}

/// Which curve types the fitter may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
  /// Lines only: each run simplified to a polygon within the tolerance.
  Lines,
  /// Lines, arcs and circles.
  Arcs,
  /// Lines, arcs, circles and cubic Béziers.
  #[default]
  Curves,
}

/// Tracing options; lengths in pixels of the downscaled image, areas in square pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Options {
  pub mode: Mode,
  /// Outline mode: trace the light pixels instead of the dark ones.
  pub invert: bool,
  /// Gaussian blur sigma before segmenting (0 = none).
  pub blur: f64,
  /// Shapes (8-connected regions) smaller than this area are dropped.
  pub specks: f64,
  /// Holes (4-connected background regions not touching the border) smaller than this area
  /// are filled.
  pub holes: f64,
  /// How far a fitted curve may stray from the pixel outline.
  pub tolerance: f64,
  /// Degrees: a bend whose angle, measured over ±2 px of outline, is smaller than this is a
  /// sharp corner (180 = straight, 90 = square corner). 0 disables corners.
  pub corner_angle: f64,
  pub fit: Fit,
  /// Images whose longer side exceeds this are downscaled (area averaging) first.
  pub max_side: u32,
  /// Centre-line mode: side branches from a junction to a free end shorter than this (px) are
  /// dropped as thinning noise. `None` = 1.5 × the typical stroke width.
  pub spurs: Option<f64>,
}

impl Default for Options {
  fn default() -> Options {
    Options {
      mode: Mode::default(),
      invert: false,
      blur: 0.0,
      specks: 16.0,
      holes: 16.0,
      tolerance: 1.0,
      corner_angle: 120.0,
      fit: Fit::Curves,
      max_side: 2000,
      spurs: None,
    }
  }
}

impl Options {
  fn check(&self) -> Result<(), TraceError> {
    let bad = |m: &str| Err(TraceError::Options(m.into()));
    if !(self.tolerance > 0.0) || !self.tolerance.is_finite() {
      return bad("tolerance must be a positive number");
    }
    if !(0.0..=180.0).contains(&self.corner_angle) {
      return bad("corner_angle must be 0–180 degrees");
    }
    if !(self.blur >= 0.0) || self.blur > 100.0 {
      return bad("blur must be 0–100 px");
    }
    if !(self.specks >= 0.0) || !(self.holes >= 0.0) {
      return bad("specks and holes must be non-negative areas");
    }
    if self.spurs.is_some_and(|v| !(v >= 0.0) || !v.is_finite()) {
      return bad("spurs must be a non-negative length");
    }
    if self.max_side == 0 {
      return bad("max_side must be at least 1");
    }
    if let Mode::Colors { count } = self.mode
      && !(1..=64).contains(&count)
    {
      return bad("colors count must be 1–64");
    }
    Ok(())
  }
}

/// The result: closed contours grouped in layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
  /// Size of the traced (downscaled) image in pixels.
  pub width: u32,
  pub height: u32,
  /// Outline mode: one layer. Colours mode: one per colour, largest area first.
  pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
  /// Mean colour of the layer's pixels.
  pub color: [u8; 3],
  /// The colour most common along the image border (Colours mode); callers usually skip it.
  pub background: bool,
  /// Each outer boundary is followed by its holes (empty in centre-line mode).
  pub contours: Vec<Contour>,
  /// Centre-line mode: the single-line strokes. Strokes meeting at a junction end at exactly
  /// the same point.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub strokes: Vec<Stroke>,
}

/// A centre line: open (from one end or junction to another), or a closed loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
  /// In order; each starts where the previous one ends. A closed stroke ends where it starts
  /// (or is a single [`Seg::Circle`]).
  pub segs: Vec<Seg>,
  pub closed: bool,
  /// The stroke's mean width (px), e.g. to pick an engraving cutter.
  pub width: f64,
}

/// One closed boundary: counter-clockwise, or clockwise when it is a hole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contour {
  pub segs: Vec<Seg>,
  pub hole: bool,
}

/// A boundary piece, in traversal order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Seg {
  Line {
    a: [f64; 2],
    b: [f64; 2],
  },
  /// Radius `|a - center|`; `b` lies on the circle up to rounding. `ccw`: from `a` to `b`
  /// counter-clockwise (y up).
  Arc {
    a: [f64; 2],
    b: [f64; 2],
    center: [f64; 2],
    ccw: bool,
  },
  /// A cubic Bézier from `a` to `b` with control points `c1`, `c2`.
  Cubic {
    a: [f64; 2],
    c1: [f64; 2],
    c2: [f64; 2],
    b: [f64; 2],
  },
  /// A whole circle, only ever a contour's sole segment; its direction follows the contour's
  /// `hole` flag. Starts and ends at `center + (radius, 0)`.
  Circle {
    center: [f64; 2],
    radius: f64,
  },
}

impl Seg {
  pub fn start(&self) -> [f64; 2] {
    match *self {
      Seg::Line { a, .. } | Seg::Arc { a, .. } | Seg::Cubic { a, .. } => a,
      Seg::Circle { center, radius } => [center[0] + radius, center[1]],
    }
  }

  pub fn end(&self) -> [f64; 2] {
    match *self {
      Seg::Line { b, .. } | Seg::Arc { b, .. } | Seg::Cubic { b, .. } => b,
      Seg::Circle { center, radius } => [center[0] + radius, center[1]],
    }
  }
}

/// Traces `image` into closed contours.
pub fn trace(image: &Image, options: &Options) -> Result<Trace, TraceError> {
  options.check()?;
  if image.width == 0
    || image.height == 0
    || image.rgba.len() as u64 != u64::from(image.width) * u64::from(image.height) * 4
  {
    return Err(TraceError::Image("pixel data doesn't match the size".into()));
  }
  let mut rgb = raster::Rgb::from_image(image, options.max_side);
  if options.blur > 0.0 {
    rgb.blur(options.blur);
  }
  let masks = match options.mode {
    Mode::Outline { threshold } => vec![segment::outline(&rgb, threshold, options.invert)],
    Mode::Colors { count } => segment::colors(&rgb, count),
    Mode::Centerline { threshold } => {
      let mut m = segment::outline(&rgb, threshold, options.invert);
      clean::clean(&mut m.mask, rgb.w, rgb.h, options.specks, options.holes);
      let strokes = centerline::strokes(&m.mask, rgb.w, rgb.h, options);
      let layer = Layer { color: m.color, background: false, contours: Vec::new(), strokes };
      return Ok(Trace { width: rgb.w as u32, height: rgb.h as u32, layers: vec![layer] });
    }
  };
  let (w, h) = (rgb.w, rgb.h);
  let layers = masks
    .into_iter()
    .map(|m| {
      let mut mask = m.mask;
      let labels = clean::clean(&mut mask, w, h, options.specks, options.holes);
      let prepared: Vec<fit::Prepared> =
        contour::trace(&mask, &labels, w, h).iter().map(|raw| fit::prepare(raw, options)).collect();
      Layer {
        color: m.color,
        background: m.background,
        contours: validate::finish(&prepared, options),
        strokes: Vec::new(),
      }
    })
    .collect();
  Ok(Trace { width: w as u32, height: h as u32, layers })
}
