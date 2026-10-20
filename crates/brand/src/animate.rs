//! The splash and the ripple loader frame by frame, for apps that can't play `brand/logo/motion/`'s
//! CSS animations (the desktop app paints with egui). Each function answers "where is every part
//! at `t` seconds?" exactly as the SVG's keyframes would, from the numbers in [`crate::motion`];
//! painting the answer is the app's job.

use crate::logo::FACETS;
use crate::motion::{NUCLEUS, ripple, splash};

/// A 2D affine transform: `(x, y)` maps to `(a x + c y + e, b x + d y + f)`, y down as in SVG.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
  pub a: f32,
  pub b: f32,
  pub c: f32,
  pub d: f32,
  pub e: f32,
  pub f: f32,
}

impl Affine {
  pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

  pub const fn translate(x: f32, y: f32) -> Affine {
    Affine { e: x, f: y, ..Affine::IDENTITY }
  }

  pub const fn scale(x: f32, y: f32) -> Affine {
    Affine { a: x, d: y, ..Affine::IDENTITY }
  }

  /// Clockwise on screen, like SVG's `rotate()`.
  pub fn rotate(degrees: f32) -> Affine {
    let (s, c) = degrees.to_radians().sin_cos();
    Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
  }

  /// `self` applied after `inner`, as CSS lists them: `outer.then(inner)` is "outer inner".
  pub fn then(self, inner: Affine) -> Affine {
    Affine {
      a: self.a * inner.a + self.c * inner.b,
      b: self.b * inner.a + self.d * inner.b,
      c: self.a * inner.c + self.c * inner.d,
      d: self.b * inner.c + self.d * inner.d,
      e: self.a * inner.e + self.c * inner.f + self.e,
      f: self.b * inner.e + self.d * inner.f + self.f,
    }
  }

  /// `self` about `origin` (CSS `transform-origin`).
  pub fn about(self, origin: (f32, f32)) -> Affine {
    Affine::translate(origin.0, origin.1).then(self).then(Affine::translate(-origin.0, -origin.1))
  }

  pub fn apply(self, (x, y): (f32, f32)) -> (f32, f32) {
    (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
  }
}

/// Where a part is and how opaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
  pub transform: Affine,
  pub opacity: f32,
}

impl Pose {
  pub const HIDDEN: Pose = Pose { transform: Affine::IDENTITY, opacity: 0.0 };
  pub const REST: Pose = Pose { transform: Affine::IDENTITY, opacity: 1.0 };
}

/// A glow or ring centred on [`NUCLEUS`]: its radius multiplier and opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
  pub scale: f32,
  pub opacity: f32,
}

/// The splash at one moment, on the [`splash::SIZE`] canvas.
#[derive(Debug, Clone)]
pub struct SplashFrame {
  /// One per [`FACETS`] entry.
  pub facets: Vec<Pose>,
  /// One per [`splash::GLYPHS`] entry, placing the glyph's `cell` on the canvas.
  pub glyphs: Vec<Pose>,
  pub flash: Pulse,
  pub ring: Pulse,
}

/// How long the splash takes to settle on the finished wordmark.
pub fn splash_duration() -> f32 {
  let shards = splash::SHARDS.iter().map(|s| s.3 + splash::SHARD_FOR).fold(0.0, f32::max);
  let letters = splash::GLYPHS
    .iter()
    .map(|g| match g.part {
      splash::Part::Pop { at, raised: true, .. } => {
        (at + splash::POP_FOR).max(splash::OPEN_AT + 0.05 + splash::SHRINK_FOR)
      }
      splash::Part::Pop { at, .. } => at + splash::POP_FOR,
      splash::Part::Unfold { at } => at + splash::UNFOLD_FOR,
    })
    .fold(0.0, f32::max);
  shards.max(letters).max(splash::OPEN_AT + splash::OPEN_FOR)
}

/// The splash `t` seconds in. From [`splash_duration`] on it is the finished wordmark.
pub fn splash(t: f32) -> SplashFrame {
  let shard_ease = cubic_bezier(0.2, 1.35, 0.4, 1.0);
  let facets = FACETS
    .iter()
    .zip(splash::SHARDS)
    .map(|(f, &(dx, dy, rot, delay))| {
      let Some(u) = progress(t, delay, splash::SHARD_FOR) else { return Pose::HIDDEN };
      // transform eases over the whole flight, opacity over its first 35 %
      let k = shard_ease(u);
      let m = Affine::translate(dx * (1.0 - k), dy * (1.0 - k))
        .then(Affine::rotate(rot * (1.0 - k)))
        .then(uniform(lerp(0.1, 1.0, k)));
      Pose { transform: m.about(bbox_centre(f.points)), opacity: shard_ease((u / 0.35).min(1.0)).clamp(0.0, 1.0) }
    })
    .collect();

  let ease = cubic_bezier(0.65, 0.0, 0.25, 1.0);
  let pop = cubic_bezier(0.3, 1.6, 0.5, 1.0);
  let unfold = cubic_bezier(0.3, 1.4, 0.5, 1.0);
  // the word sits centred and the O and x slid left until the name opens out
  let open = progress(t, splash::OPEN_AT, splash::OPEN_FOR).map_or(0.0, &ease);
  let text = Affine::translate(splash::TEXT_ORIGIN.0 + splash::CENTRE * (1.0 - open), splash::TEXT_ORIGIN.1);
  let glyphs = splash::GLYPHS
    .iter()
    .map(|g| {
      let foot = ((g.bounds.0 + g.bounds.2) / 2.0, g.bounds.3);
      let (own, opacity, slides) = match g.part {
        splash::Part::Pop { at, slides, raised } => {
          let Some(u) = progress(t, at, splash::POP_FOR) else { return Pose::HIDDEN };
          let k = pop(u);
          let to = if raised { splash::X_SCALE } else { 1.0 };
          let mut m =
            Affine::translate(0.0, 60.0 * (1.0 - k)).then(uniform(lerp(if raised { 0.8 } else { 0.6 }, to, k)));
          if raised && let Some(u) = progress(t, splash::OPEN_AT + 0.05, splash::SHRINK_FOR) {
            m = uniform(lerp(splash::X_SCALE, 1.0, ease(u)));
          }
          (m, k.clamp(0.0, 1.0), slides)
        }
        splash::Part::Unfold { at } => {
          let Some(u) = progress(t, at, splash::UNFOLD_FOR) else { return Pose::HIDDEN };
          let k = unfold(u);
          (
            Affine::scale(lerp(0.2, 1.0, k), 1.0).then(Affine::translate(0.0, 20.0 * (1.0 - k))),
            k.clamp(0.0, 1.0),
            false,
          )
        }
      };
      let slide = if slides { Affine::translate(splash::SHIFT * (1.0 - open), 0.0) } else { Affine::IDENTITY };
      Pose { transform: text.then(slide).then(own.about(foot)), opacity }
    })
    .collect();

  let ease_out = cubic_bezier(0.0, 0.0, 0.58, 1.0);
  let flash = match progress(t, 0.0, splash::FLASH_FOR) {
    Some(u) => {
      let [opacity, scale] =
        keyframes(&[(0.0, [0.0, 0.2]), (0.3, [1.0, 1.0]), (0.55, [1.0, 0.7]), (1.0, [0.0, 3.2])], u, &ease_out);
      Pulse { scale, opacity }
    }
    None => Pulse { scale: 0.2, opacity: 0.0 },
  };
  let ring = match progress(t, splash::RING_AT, splash::RING_FOR) {
    Some(u) => {
      let k = ease_out(u);
      Pulse { scale: lerp(0.1, 2.6, k), opacity: lerp(0.8, 0.0, k) }
    }
    // `both`: the ring waits at its first frame
    None => Pulse { scale: 0.1, opacity: 0.8 },
  };
  SplashFrame { facets, glyphs, flash, ring }
}

/// The ripple loader at one moment, on a canvas the fox's size plus [`ripple::PAD`] all round.
#[derive(Debug, Clone)]
pub struct RippleFrame {
  /// One per [`FACETS`] entry.
  pub facets: Vec<Pose>,
  /// The glow at the nose.
  pub nose: Pulse,
}

/// The ripple loader's canvas size.
pub fn ripple_size() -> (f32, f32) {
  (crate::logo::SIZE.0 + 2.0 * ripple::PAD, crate::logo::SIZE.1 + 2.0 * ripple::PAD)
}

/// The ripple loader `t` seconds in; it loops every [`ripple::PERIOD`].
pub fn ripple(t: f32) -> RippleFrame {
  let in_out = cubic_bezier(0.42, 0.0, 0.58, 1.0);
  let pad = Affine::translate(ripple::PAD, ripple::PAD);
  let facets = FACETS
    .iter()
    .zip(ripple::LAG)
    .map(|(f, lag)| {
      let u = ((t - lag) / ripple::PERIOD).rem_euclid(1.0);
      let [opacity, scale] = keyframes(&pairs(ripple::FACET), u, &in_out);
      Pose { transform: pad.then(uniform(scale).about(bbox_centre(f.points))), opacity }
    })
    .collect();
  let u = (t / ripple::PERIOD).rem_euclid(1.0);
  let [opacity, scale] = keyframes(&pairs(ripple::NOSE), u, &cubic_bezier(0.0, 0.0, 0.58, 1.0));
  RippleFrame { facets, nose: Pulse { scale, opacity } }
}

/// Where the nose glow sits on the ripple canvas.
pub fn ripple_nucleus() -> (f32, f32) {
  (NUCLEUS.0 + ripple::PAD, NUCLEUS.1 + ripple::PAD)
}

fn pairs(keys: &[(f32, f32, f32)]) -> Vec<(f32, [f32; 2])> {
  keys.iter().map(|&(at, a, b)| (at, [a, b])).collect()
}

/// An animation's progress 0..1 at `t`, or `None` before it starts (it then shows its first
/// keyframe, CSS `fill-mode: both`); after it ends it holds at 1.
fn progress(t: f32, start: f32, duration: f32) -> Option<f32> {
  (t >= start).then(|| ((t - start) / duration).min(1.0))
}

/// Values at progress `u` through `keys`, each segment eased separately as CSS does.
fn keyframes<const N: usize>(keys: &[(f32, [f32; N])], u: f32, ease: &impl Fn(f32) -> f32) -> [f32; N] {
  for w in keys.windows(2) {
    let ((a, va), (b, vb)) = (w[0], w[1]);
    if u <= b {
      let k = ease((u - a) / (b - a));
      return std::array::from_fn(|i| lerp(va[i], vb[i], k));
    }
  }
  keys.last().map(|k| k.1).unwrap_or([0.0; N])
}

/// A CSS `cubic-bezier()` timing function. Its output may overshoot 0..1.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 {
  move |x: f32| {
    if x <= 0.0 {
      return 0.0;
    }
    if x >= 1.0 {
      return 1.0;
    }
    let curve =
      |p1: f32, p2: f32, t: f32| 3.0 * (1.0 - t) * (1.0 - t) * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t * t * t;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
      let mid = (lo + hi) / 2.0;
      if curve(x1, x2, mid) < x {
        lo = mid;
      } else {
        hi = mid;
      }
    }
    curve(y1, y2, (lo + hi) / 2.0)
  }
}

fn uniform(s: f32) -> Affine {
  Affine::scale(s, s)
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
  a + (b - a) * k
}

/// The centre of a facet's bounding box (CSS `transform-box: fill-box; transform-origin: center`).
fn bbox_centre(points: &[(f32, f32)]) -> (f32, f32) {
  let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
  for &(x, y) in points {
    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
  }
  ((x0 + x1) / 2.0, (y0 + y1) / 2.0)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn close(a: Affine, b: Affine) -> bool {
    [a.a - b.a, a.b - b.b, a.c - b.c, a.d - b.d, a.e - b.e, a.f - b.f].iter().all(|v| v.abs() < 1e-3)
  }

  #[test]
  fn the_splash_ends_on_the_wordmark() {
    let end = splash(splash_duration());
    assert!(end.facets.iter().all(|p| p.opacity == 1.0 && close(p.transform, Affine::IDENTITY)));
    let text = Affine::translate(splash::TEXT_ORIGIN.0, splash::TEXT_ORIGIN.1);
    assert!(end.glyphs.iter().all(|p| p.opacity == 1.0 && close(p.transform, text)));
    assert_eq!(end.flash.opacity, 0.0);
    assert_eq!(end.ring.opacity, 0.0);
    assert!((2.5..3.5).contains(&splash_duration()));
  }

  #[test]
  fn the_splash_starts_from_nothing() {
    let start = splash(0.0);
    assert!(start.facets.iter().chain(&start.glyphs).all(|p| p.opacity == 0.0));
    // "FOX" before the name opens: the O sits right after the F, the x is capital height
    let mid = splash(splash::OPEN_AT);
    let shown: String =
      splash::GLYPHS.iter().zip(&mid.glyphs).filter(|(_, p)| p.opacity > 0.5).map(|(g, _)| g.ch).collect();
    assert_eq!(shown, "FOx");
    let o = splash::GLYPHS.iter().position(|g| g.ch == 'O').unwrap();
    let left = mid.glyphs[o].transform.apply((splash::GLYPHS[o].bounds.0, 0.0)).0;
    let f_right = mid.glyphs[0].transform.apply((splash::GLYPHS[0].bounds.2, 0.0)).0;
    assert!(left > f_right && left - f_right < 40.0, "O at {left}, F ends at {f_right}");
  }

  #[test]
  fn the_ripple_loops() {
    let (a, b) = (ripple(0.3), ripple(0.3 + ripple::PERIOD));
    for (p, q) in a.facets.iter().zip(&b.facets) {
      assert!((p.opacity - q.opacity).abs() < 1e-3 && close(p.transform, q.transform));
    }
    assert_eq!(a.facets.len(), FACETS.len());
    // every facet dims once a loop, the ones near the nose first
    let frames: Vec<RippleFrame> = (0..32).map(|i| ripple(i as f32 * ripple::PERIOD / 32.0)).collect();
    assert!((0..FACETS.len()).all(|f| frames.iter().any(|r| r.facets[f].opacity < 0.5)));
  }

  #[test]
  fn timing_functions_match_css() {
    let ease = cubic_bezier(0.25, 0.1, 0.25, 1.0);
    assert!((ease(0.5) - 0.8024).abs() < 1e-3);
    let linear = cubic_bezier(0.0, 0.0, 1.0, 1.0);
    assert!((linear(0.3) - 0.3).abs() < 1e-3);
    // overshoot is kept: the shards fly a little past their place and settle back
    let shard = cubic_bezier(0.2, 1.35, 0.4, 1.0);
    assert!((1..20).any(|i| shard(i as f32 / 20.0) > 1.0));
  }
}
