//! Text along a path: each glyph keeps its shape and stands on the curve, the middle of its
//! advance on the path and its baseline along the path's tangent there (as Fusion's "along path"
//! text does). The path is read in whichever direction keeps the text upright, so text on top of
//! an arc reads left to right whichever way the arc was drawn.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::{CurveId, Seg, Sketch};

/// Pieces a Bézier is cut into for arc length.
const BEZIER_STEPS: usize = 64;
/// Pieces an elliptical arc is cut into per full turn.
const ELLIPSE_STEPS: f64 = 256.0;

/// Where a text's glyphs go along a curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextPath {
  /// The curve the text follows.
  pub curve: CurveId,
  /// The glyphs stand on the path (true) or hang below it, their capitals' tops on it.
  #[serde(default = "yes")]
  pub above: bool,
  #[serde(default)]
  pub align: TextAlign,
}

fn yes() -> bool {
  true
}

/// Where a path text sits along its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
  /// Starting at the path's start.
  #[default]
  Left,
  /// Centred on the path.
  Center,
  /// Ending at the path's end.
  Right,
}

impl TextAlign {
  pub fn parse(s: &str) -> Option<TextAlign> {
    match s {
      "left" => Some(TextAlign::Left),
      "center" | "centre" => Some(TextAlign::Center),
      "right" => Some(TextAlign::Right),
      _ => None,
    }
  }
}

/// One piece of a path, with exact arc length.
#[derive(Clone, Copy)]
enum Piece {
  /// From `a` along the unit direction `d` for `len`.
  Line { a: [f64; 2], d: [f64; 2], len: f64 },
  Arc { c: [f64; 2], r: f64, a0: f64, sweep: f64 },
}

impl Piece {
  fn len(&self) -> f64 {
    match *self {
      Piece::Line { len, .. } => len,
      Piece::Arc { r, sweep, .. } => r * sweep.abs(),
    }
  }

  /// Point and unit tangent `s` along the piece.
  fn at(&self, s: f64) -> ([f64; 2], [f64; 2]) {
    match *self {
      Piece::Line { a, d, .. } => ([a[0] + d[0] * s, a[1] + d[1] * s], d),
      Piece::Arc { c, r, a0, sweep } => {
        let a = a0 + sweep.signum() * s / r;
        let (sn, cs) = a.sin_cos();
        ([c[0] + r * cs, c[1] + r * sn], [-sn * sweep.signum(), cs * sweep.signum()])
      }
    }
  }

  fn reversed(&self) -> Piece {
    match *self {
      Piece::Line { a, d, len } => Piece::Line { a: [a[0] + d[0] * len, a[1] + d[1] * len], d: [-d[0], -d[1]], len },
      Piece::Arc { c, r, a0, sweep } => Piece::Arc { c, r, a0: a0 + sweep, sweep: -sweep },
    }
  }
}

fn line(a: [f64; 2], b: [f64; 2], out: &mut Vec<Piece>) {
  let len = crate::dist(a, b);
  if len > 1e-12 {
    out.push(Piece::Line { a, d: [(b[0] - a[0]) / len, (b[1] - a[1]) / len], len });
  }
}

fn polyline(pts: &[[f64; 2]], out: &mut Vec<Piece>) {
  for w in pts.windows(2) {
    line(w[0], w[1], out);
  }
}

/// A curve measured by arc length, read in its upright direction.
pub(crate) struct PathFrame {
  pieces: Vec<Piece>,
  len: f64,
}

impl PathFrame {
  pub(crate) fn new(sketch: &Sketch, curve: CurveId) -> Option<PathFrame> {
    let mut pieces = Vec::new();
    for seg in sketch.curve_segs(curve) {
      match seg {
        Seg::Line { a, b } => line(a, b, &mut pieces),
        Seg::Arc { center, radius, start_angle, sweep } => {
          pieces.push(Piece::Arc { c: center, r: radius, a0: start_angle, sweep })
        }
        Seg::Circle { center, radius, ccw } => {
          pieces.push(Piece::Arc { c: center, r: radius, a0: 0.0, sweep: if ccw { TAU } else { -TAU } })
        }
        Seg::Bezier { p } => {
          let pts: Vec<[f64; 2]> =
            (0..=BEZIER_STEPS).map(|i| crate::bezier_at(&p, i as f64 / BEZIER_STEPS as f64)).collect();
          polyline(&pts, &mut pieces);
        }
        Seg::EllipseArc { center, major, minor, start, sweep } => {
          let n = ((sweep.abs() / TAU) * ELLIPSE_STEPS).ceil().max(8.0) as usize;
          let pts: Vec<[f64; 2]> = (0..=n)
            .map(|i| crate::profile::ellipse_point(center, major, minor, start + sweep * i as f64 / n as f64))
            .collect();
          polyline(&pts, &mut pieces);
        }
      }
    }
    pieces.retain(|p| p.len() > 1e-12);
    let len: f64 = pieces.iter().map(Piece::len).sum();
    if pieces.is_empty() || !len.is_finite() {
      return None;
    }
    let mut f = PathFrame { pieces, len };
    // read it so the text's up (left of the travel direction) points up at the path's middle
    let (_, t) = f.at(len / 2.0);
    if t[0] < -1e-9 {
      f.pieces = f.pieces.iter().rev().map(Piece::reversed).collect();
    }
    Some(f)
  }

  pub(crate) fn len(&self) -> f64 {
    self.len
  }

  /// Point and unit tangent `s` along the path; beyond its ends the path runs on straight.
  pub(crate) fn at(&self, s: f64) -> ([f64; 2], [f64; 2]) {
    let mut left = s;
    for (i, p) in self.pieces.iter().enumerate() {
      let l = p.len();
      if left <= l || i + 1 == self.pieces.len() {
        if left < 0.0 || left > l {
          let end = left.clamp(0.0, l);
          let (q, t) = p.at(end);
          let d = left - end;
          return ([q[0] + t[0] * d, q[1] + t[1] * d], t);
        }
        return p.at(left);
      }
      left -= l;
    }
    ([0.0, 0.0], [1.0, 0.0])
  }
}
