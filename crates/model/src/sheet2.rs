//! More sheet metal: lofted flanges and their developed flat patterns, miter flange sections,
//! corner reliefs, and flat patterns as editable sketches.
//!
//! A lofted flange joins two profiles with a ruled sheet: matching points of the two profiles
//! (by arc length, with the corners of either kept as facet boundaries) are joined by straight
//! lines. A die-formed flange is one smooth surface through them, a press-brake one is flat
//! facets with straight bends between them. The B-rep unfolder only handles planar faces and
//! cylindrical bends, so a lofted flange carries its own flat pattern, developed triangle by
//! triangle on the sheet's neutral layer.

use std::f64::consts::PI;

use fox_kernel::glam::DVec3;
use fox_kernel::{Bend, Plane, Shape};
use fox_sketch::{Loop, Profile, PtSpec, Seg, Sketch};

use crate::sheet::{self, FlatPattern, SheetInfo};
use crate::{CornerRelief, LoftMethod};

type P2 = [f64; 2];

fn sub(a: P2, b: P2) -> P2 {
  [a[0] - b[0], a[1] - b[1]]
}

fn len2(a: P2) -> f64 {
  a[0].hypot(a[1])
}

fn cross2(a: P2, b: P2) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

fn polar(c: P2, r: f64, a: f64) -> P2 {
  [c[0] + r * a.cos(), c[1] + r * a.sin()]
}

// ---------------------------------------------------------------- profile chains

/// A chain of sketch segments in 3D, measured by arc length.
pub struct Chain {
  pub segs: Vec<Seg>,
  pub closed: bool,
  pub plane: Plane,
}

/// Points along a Bézier piece for measuring it.
const BEZIER_STEPS: usize = 32;

fn seg_len(s: &Seg) -> f64 {
  match *s {
    Seg::Line { a, b } => len2(sub(b, a)),
    Seg::Arc { radius, sweep, .. } => radius * sweep.abs(),
    Seg::Circle { radius, .. } => 2.0 * PI * radius,
    Seg::Bezier { p } => {
      let pts: Vec<P2> =
        (0..=BEZIER_STEPS).map(|i| fox_sketch::bezier_at(&p, i as f64 / BEZIER_STEPS as f64)).collect();
      pts.windows(2).map(|w| len2(sub(w[1], w[0]))).sum()
    }
  }
}

/// The point a fraction `f` of the way along a segment (by length; Béziers approximately).
fn seg_at(s: &Seg, f: f64) -> P2 {
  match *s {
    Seg::Line { a, b } => [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f],
    Seg::Arc { center, radius, start_angle, sweep } => polar(center, radius, start_angle + sweep * f),
    Seg::Circle { center, radius, ccw } => polar(center, radius, if ccw { 2.0 * PI * f } else { -2.0 * PI * f }),
    Seg::Bezier { p } => {
      let pts: Vec<P2> =
        (0..=BEZIER_STEPS).map(|i| fox_sketch::bezier_at(&p, i as f64 / BEZIER_STEPS as f64)).collect();
      let cum: Vec<f64> = std::iter::once(0.0)
        .chain(pts.windows(2).scan(0.0, |acc, w| {
          *acc += len2(sub(w[1], w[0]));
          Some(*acc)
        }))
        .collect();
      let target = f * cum[BEZIER_STEPS];
      let i = cum.iter().position(|c| *c >= target).unwrap_or(BEZIER_STEPS).clamp(1, BEZIER_STEPS);
      let span = (cum[i] - cum[i - 1]).max(1e-12);
      let k = ((target - cum[i - 1]) / span).clamp(0.0, 1.0);
      [pts[i - 1][0] + (pts[i][0] - pts[i - 1][0]) * k, pts[i - 1][1] + (pts[i][1] - pts[i - 1][1]) * k]
    }
  }
}

impl Chain {
  fn lengths(&self) -> Vec<f64> {
    self.segs.iter().map(seg_len).collect()
  }

  /// The point at arc length `s` from the start (wrapped on closed chains).
  fn at(&self, lens: &[f64], s: f64) -> DVec3 {
    let total: f64 = lens.iter().sum();
    let mut s = if self.closed { s.rem_euclid(total) } else { s.clamp(0.0, total) };
    for (seg, l) in self.segs.iter().zip(lens) {
      if s <= *l || std::ptr::eq(seg, self.segs.last().unwrap_or(seg)) {
        return self.plane.to_world(seg_at(seg, if *l > 1e-12 { (s / l).clamp(0.0, 1.0) } else { 0.0 }));
      }
      s -= l;
    }
    self.plane.to_world(self.segs.last().map(|g| g.end()).unwrap_or([0.0, 0.0]))
  }

  /// Arc lengths of the sharp corners between segments (0 for a closed chain's start corner).
  fn corners(&self, lens: &[f64]) -> Vec<f64> {
    let tangent = |s: &Seg, at_end: bool| {
      let (a, b) = if at_end { (seg_at(s, 1.0 - 1e-4), s.end()) } else { (s.start(), seg_at(s, 1e-4)) };
      let d = sub(b, a);
      let l = len2(d);
      [d[0] / l.max(1e-15), d[1] / l.max(1e-15)]
    };
    let mut out = Vec::new();
    let mut s = 0.0;
    let n = self.segs.len();
    for (i, l) in lens.iter().enumerate().take(n) {
      if i + 1 < n || self.closed {
        let (t0, t1) = (tangent(&self.segs[i], true), tangent(&self.segs[(i + 1) % n], false));
        if cross2(t0, t1).abs() > 0.017 || t0[0] * t1[0] + t0[1] * t1[1] < 0.0 {
          out.push(if i + 1 < n { s + l } else { 0.0 });
        }
      }
      s += l;
    }
    out
  }
}

/// How a profile is read: from `start` along the chain (`dir` ±1), over arc lengths lo..hi of
/// that walk, scaled to u in 0..1.
struct Walk<'a> {
  chain: &'a Chain,
  lens: Vec<f64>,
  total: f64,
  start: f64,
  dir: f64,
  lo: f64,
  hi: f64,
}

impl Walk<'_> {
  fn at(&self, u: f64) -> DVec3 {
    let x = self.lo + u * (self.hi - self.lo);
    let s = if self.chain.closed {
      self.start + self.dir * x
    } else if self.dir > 0.0 {
      x
    } else {
      self.total - x
    };
    self.chain.at(&self.lens, s)
  }

  /// The chain's corners as u values inside (0, 1).
  fn corner_us(&self) -> Vec<f64> {
    self
      .chain
      .corners(&self.lens)
      .into_iter()
      .filter_map(|c| {
        let x = if self.chain.closed {
          (self.dir * (c - self.start)).rem_euclid(self.total)
        } else if self.dir > 0.0 {
          c
        } else {
          self.total - c
        };
        let u = (x - self.lo) / (self.hi - self.lo);
        (u > 1e-9 && u < 1.0 - 1e-9).then_some(u)
      })
      .collect()
  }

  /// Whether the profile is straight between u0 and u1.
  fn straight(&self, u0: f64, u1: f64) -> bool {
    let (a, b) = (self.at(u0), self.at(u1));
    let d = b - a;
    let l = d.length();
    (1..6).all(|k| {
      let p = self.at(u0 + (u1 - u0) * k as f64 / 6.0);
      let v = p - a;
      (v - d * (v.dot(d) / (l * l).max(1e-18))).length() < 1e-6 * (1.0 + l)
    })
  }
}

// ---------------------------------------------------------------- lofted flange

/// A lofted flange's flat pattern, developed when the flange is built.
#[derive(Debug, Clone)]
pub struct Development {
  /// The flat's top face lies on this plane, the material behind it.
  pub plane: Plane,
  pub outline: Vec<P2>,
  pub bends: Vec<Bend>,
  /// Volume of the flange as built (later changes to the body aren't in the flat pattern).
  pub volume: f64,
}

impl PartialEq for Development {
  fn eq(&self, other: &Self) -> bool {
    self.outline == other.outline && self.volume == other.volume
  }
}

impl Development {
  /// The flat pattern of a sheet `t` thick.
  pub fn flat_pattern(&self, t: f64) -> Result<FlatPattern, String> {
    let n = self.outline.len();
    let segs = (0..n).map(|i| Seg::Line { a: self.outline[i], b: self.outline[(i + 1) % n] }).collect();
    let mut outer = Loop { segs };
    if outer.signed_area() < 0.0 {
      outer = outer.reversed();
    }
    let face = Shape::face(&self.plane, &Profile { outer, holes: Vec::new() })
      .map_err(|e| format!("the developed flat pattern overlaps itself ({e})"))?;
    let shape = face.extrude(-self.plane.normal() * t).map_err(|e| e.to_string())?;
    Ok(FlatPattern { shape, bends: self.bends.clone(), plane: self.plane })
  }
}

/// Facets of a die-formed flange (along the profiles).
const DIE_FACETS: f64 = 96.0;

/// Builds a lofted flange between two profiles. The profiles are one face of the sheet, the
/// material lies along (a × b) where a runs along the first profile and b from it to the
/// second (the other way when `flip`). `bends` is the press-brake bend count to aim for.
pub fn lofted_flange(
  a: &Chain,
  b: &Chain,
  method: LoftMethod,
  bends: usize,
  flip: bool,
  sheet: &SheetInfo,
) -> Result<(Shape, Development), String> {
  let (t, k) = (sheet.thickness, sheet.k_factor);
  if a.closed != b.closed {
    return Err("the profiles must both be closed or both open".into());
  }
  let (pa, pb, sharp) = match tangent_pairs(a, b, method, bends, sheet.gap) {
    Some(r) => r?,
    None => arc_length_pairs(a, b, method, bends, sheet.gap)?,
  };
  // a die-formed flange is one smooth surface unless a profile has corners; then fine facets
  let smooth = method == LoftMethod::DieFormed && !sharp;
  let strip = Strip::new(pa, pb, flip, smooth)?;
  let shape = if smooth { strip.smooth_solid(t)? } else { strip.solid(t)? };
  let volume = shape.mass().map_err(|e| e.to_string())?.volume;
  let mut dev = strip.develop(t, k, method == LoftMethod::PressBrake)?;
  dev.volume = volume;
  Ok((shape, dev))
}

/// Angle between samples of a die-formed flange's curved parts.
const DIE_STEP: f64 = PI / 90.0;

/// A piece of a convex closed profile, by its tangent direction φ (counter-clockwise, unwrapped).
#[derive(Debug, Clone, Copy)]
enum El {
  Straight { phi: f64, p0: P2, p1: P2 },
  Curve { phi0: f64, phi1: f64, c: P2, r: f64 },
  Corner { phi0: f64, phi1: f64, p: P2 },
}

impl El {
  fn range(&self) -> (f64, f64) {
    match *self {
      El::Straight { phi, .. } => (phi, phi),
      El::Curve { phi0, phi1, .. } | El::Corner { phi0, phi1, .. } => (phi0, phi1),
    }
  }

  fn start(&self) -> P2 {
    match *self {
      El::Straight { p0, .. } => p0,
      El::Curve { phi0, c, r, .. } => polar(c, r, phi0 - PI / 2.0),
      El::Corner { p, .. } => p,
    }
  }

  fn at(&self, phi: f64) -> P2 {
    match *self {
      El::Straight { p0, .. } => p0,
      El::Curve { c, r, .. } => polar(c, r, phi - PI / 2.0),
      El::Corner { p, .. } => p,
    }
  }

  fn shifted(self, d: f64) -> El {
    match self {
      El::Straight { phi, p0, p1 } => El::Straight { phi: phi + d, p0, p1 },
      El::Curve { phi0, phi1, c, r } => El::Curve { phi0: phi0 + d, phi1: phi1 + d, c, r },
      El::Corner { phi0, phi1, p } => El::Corner { phi0: phi0 + d, phi1: phi1 + d, p },
    }
  }
}

/// A closed counter-clockwise profile as pieces by tangent direction, if it is convex.
fn tangent_elements(segs: &[Seg]) -> Option<Vec<El>> {
  let mut items = Vec::new();
  for s in segs {
    match *s {
      Seg::Line { a, b } => items.push(El::Straight { phi: (b[1] - a[1]).atan2(b[0] - a[0]), p0: a, p1: b }),
      Seg::Arc { center, radius, start_angle, sweep } if sweep > 0.0 => {
        let phi0 = start_angle + PI / 2.0;
        items.push(El::Curve { phi0, phi1: phi0 + sweep, c: center, r: radius });
      }
      Seg::Circle { center, radius, ccw: true } => {
        items.push(El::Curve { phi0: PI / 2.0, phi1: PI / 2.0 + 2.0 * PI, c: center, r: radius })
      }
      Seg::Bezier { p } => {
        let pts: Vec<P2> = (0..=16).map(|i| fox_sketch::bezier_at(&p, i as f64 / 16.0)).collect();
        for w in pts.windows(2) {
          if len2(sub(w[1], w[0])) > 1e-9 {
            items.push(El::Straight { phi: (w[1][1] - w[0][1]).atan2(w[1][0] - w[0][0]), p0: w[0], p1: w[1] });
          }
        }
      }
      _ => return None,
    }
  }
  let first = *items.first()?;
  let mut out: Vec<El> = Vec::new();
  let mut end: Option<f64> = None;
  for it in items {
    let (s0, _) = it.range();
    let it = match end {
      None => it,
      Some(e) => {
        let mut turn = (s0 - e).rem_euclid(2.0 * PI);
        if turn > 2.0 * PI - 1e-7 {
          turn = 0.0;
        }
        if turn > PI + 1e-9 {
          return None;
        }
        if turn > 1e-9 {
          out.push(El::Corner { phi0: e, phi1: e + turn, p: it.start() });
        }
        it.shifted(e + turn - s0)
      }
    };
    end = Some(it.range().1);
    out.push(it);
  }
  // back to the start: one full turn in all
  let (f0, e) = (out[0].range().0, end?);
  let closing = f0 + 2.0 * PI - e;
  if !(-1e-7..=PI + 1e-9).contains(&closing) {
    return None;
  }
  if closing > 1e-9 {
    out.push(El::Corner { phi0: e, phi1: f0 + 2.0 * PI, p: first.start() });
  }
  Some(out)
}

/// The point of a profile with tangent direction φ (and the straight there, if any).
fn tangent_at(els: &[El], phi: f64) -> (P2, Option<(P2, P2)>) {
  let base = els[0].range().0;
  let phi = base + (phi - base).rem_euclid(2.0 * PI);
  let near = |a: f64, b: f64| (a - b).abs() < 1e-9 || (a - b).abs() > 2.0 * PI - 1e-9;
  for e in els {
    if let El::Straight { phi: p, p0, p1 } = *e
      && near(p, phi)
    {
      return (p0, Some((p0, p1)));
    }
  }
  let found = els.iter().find(|e| {
    let (a, b) = e.range();
    phi >= a - 1e-12 && phi <= b + 1e-12
  });
  (found.unwrap_or(&els[0]).at(phi), None)
}

/// Matches two closed convex profiles in parallel planes by tangent direction: each point of one
/// joins the point of the other where the profiles run the same way. That makes the surface
/// developable (as sheet metal must be): straight sides join to points in flat triangles, corners
/// to curves in cones. `None` when the profiles don't qualify.
fn tangent_pairs(
  a: &Chain,
  b: &Chain,
  method: LoftMethod,
  bends: usize,
  gap: f64,
) -> Option<Result<(Vec<DVec3>, Vec<DVec3>, bool), String>> {
  let (na, nb) = (a.plane.normal(), b.plane.normal());
  if !(a.closed && b.closed) || na.dot(nb).abs() < 1.0 - 1e-9 {
    return None;
  }
  // both in the first profile's plane, counter-clockwise
  let pl = a.plane;
  let to_a = |p: P2| pl.to_local(b.plane.to_world(p));
  let sb: Vec<Seg> = b.segs.iter().map(|s| map_seg(s, &to_a, na.dot(nb) < 0.0)).collect();
  let ccw = |segs: Vec<Seg>| {
    let l = Loop { segs };
    if l.signed_area() < 0.0 { (l.reversed().segs, true) } else { (l.segs, false) }
  };
  let (sa, flipped) = ccw(a.segs.clone());
  let (sb, _) = ccw(sb);
  let (ea, eb) = (tangent_elements(&sa)?, tangent_elements(&sb)?);
  let hb = (b.plane.origin - pl.origin).dot(na);
  let f0 = ea[0].range().0;
  let into = |x: f64| f0 + (x - f0).rem_euclid(2.0 * PI);
  let mut events: Vec<f64> = ea.iter().chain(&eb).flat_map(|e| [into(e.range().0), into(e.range().1)]).collect();
  events.push(f0);
  events.sort_by(f64::total_cmp);
  events.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
  events.retain(|e| *e < f0 + 2.0 * PI - 1e-9);
  // facets: one per straight, the rest spread over the curved stretches
  let ends: Vec<(f64, f64)> =
    events.iter().enumerate().map(|(i, e)| (*e, events.get(i + 1).copied().unwrap_or(f0 + 2.0 * PI))).collect();
  let corner = |els: &[El], phi: f64| {
    let base = els[0].range().0;
    let phi = base + (phi - base).rem_euclid(2.0 * PI);
    els.iter().any(|e| matches!(e, El::Corner { phi0, phi1, .. } if phi > *phi0 && phi < *phi1))
  };
  let moving = |(e0, e1): (f64, f64)| {
    let mid = (e0 + e1) / 2.0;
    !(corner(&ea, mid) && corner(&eb, mid))
  };
  let straights =
    events.iter().filter(|e| tangent_at(&ea, **e).1.is_some() || tangent_at(&eb, **e).1.is_some()).count();
  let curved: f64 = ends.iter().filter(|w| moving(**w)).map(|w| w.1 - w.0).sum();
  let facets = (bends + 1).saturating_sub(straights).max(1) as f64;
  let mut pairs: Vec<(P2, P2)> = Vec::new();
  let same = |x: &(P2, P2), y: &(P2, P2)| len2(sub(x.0, y.0)) < 1e-9 && len2(sub(x.1, y.1)) < 1e-9;
  let push = |pairs: &mut Vec<(P2, P2)>, p: (P2, P2)| {
    if pairs.last().is_none_or(|q| !same(q, &p)) {
      pairs.push(p);
    }
  };
  // the longest straight (where the seam goes): index of its first pair, length
  let mut longest: Option<(usize, f64)> = None;
  for (e0, e1) in ends {
    let (pa, sa) = tangent_at(&ea, e0);
    let (pb, sb) = tangent_at(&eb, e0);
    match (sa, sb) {
      (None, None) => push(&mut pairs, (pa, pb)),
      (sa, sb) => {
        let (a0, a1) = sa.unwrap_or((pa, pa));
        let (b0, b1) = sb.unwrap_or((pb, pb));
        let l = len2(sub(a1, a0)).max(len2(sub(b1, b0)));
        push(&mut pairs, (a0, b0));
        if longest.is_none_or(|(_, m)| l > m) {
          longest = Some((pairs.len() - 1, l));
        }
        push(&mut pairs, (a1, b1));
      }
    }
    if !moving((e0, e1)) {
      continue;
    }
    let span = e1 - e0;
    let m = match method {
      LoftMethod::DieFormed => (span / DIE_STEP).ceil().max(1.0) as usize,
      LoftMethod::PressBrake => (facets * span / curved.max(1e-12)).round().max(1.0) as usize,
    };
    for j in 1..m {
      let phi = e0 + span * j as f64 / m as f64;
      push(&mut pairs, (tangent_at(&ea, phi).0, tangent_at(&eb, phi).0));
    }
  }
  if pairs.len() > 1 && same(&pairs[0], &pairs[pairs.len() - 1]) {
    pairs.pop();
  }
  if pairs.len() < 3 {
    return Some(Err("the profiles are too small".into()));
  }
  // open at a seam: the middle of the longest straight, else the start
  let n = pairs.len();
  let (seam, from, count) = match longest {
    Some((i, _)) if i + 1 < n => ((mid2(pairs[i].0, pairs[i + 1].0), mid2(pairs[i].1, pairs[i + 1].1)), i + 1, n),
    _ => (pairs[0], 1, n - 1),
  };
  let mut open = vec![seam];
  open.extend((0..count).map(|k| pairs[(from + k) % n]));
  open.push(seam);
  let to3 = |p: P2, h: f64| pl.to_world(p) + na * h;
  let mut pa: Vec<DVec3> = open.iter().map(|p| to3(p.0, 0.0)).collect();
  let mut pb: Vec<DVec3> = open.iter().map(|p| to3(p.1, hb)).collect();
  if flipped {
    pa.reverse();
    pb.reverse();
  }
  let sharp = ea.iter().chain(&eb).any(|e| !matches!(e, El::Curve { .. }));
  Some(trim_seam(pa, pb, gap).map(|(pa, pb)| (pa, pb, sharp)))
}

fn mid2(a: P2, b: P2) -> P2 {
  [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// Cuts the seam gap out of a closed strip (its first and last pairs coincide): half the gap
/// along each profile from both ends.
fn trim_seam(pa: Vec<DVec3>, pb: Vec<DVec3>, gap: f64) -> Result<(Vec<DVec3>, Vec<DVec3>), String> {
  let cum = |row: &[DVec3]| -> Vec<f64> {
    let mut c = vec![0.0];
    for w in row.windows(2) {
      c.push(c[c.len() - 1] + (w[1] - w[0]).length());
    }
    c
  };
  let (ca, cb) = (cum(&pa), cum(&pb));
  let (ta, tb) = (ca[ca.len() - 1], cb[cb.len() - 1]);
  let h = gap / 2.0;
  if ta < 4.0 * h || tb < 4.0 * h {
    return Err("a profile is too short for the seam gap".into());
  }
  let at = |row: &[DVec3], c: &[f64], s: f64| {
    let i = c.iter().position(|x| *x >= s).unwrap_or(c.len() - 1).max(1);
    let k = ((s - c[i - 1]) / (c[i] - c[i - 1]).max(1e-12)).clamp(0.0, 1.0);
    row[i - 1] + (row[i] - row[i - 1]) * k
  };
  let keep = |i: usize| ca[i] > h + 1e-9 && cb[i] > h + 1e-9 && ca[i] < ta - h - 1e-9 && cb[i] < tb - h - 1e-9;
  let mut a = vec![at(&pa, &ca, h)];
  let mut b = vec![at(&pb, &cb, h)];
  for i in (0..pa.len()).filter(|i| keep(*i)) {
    a.push(pa[i]);
    b.push(pb[i]);
  }
  a.push(at(&pa, &ca, ta - h));
  b.push(at(&pb, &cb, tb - h));
  Ok((a, b))
}

/// Matches the profiles by arc length (keeping the corners of either as facet boundaries).
/// Returns the matched points and whether a profile has corners.
fn arc_length_pairs(
  a: &Chain,
  b: &Chain,
  method: LoftMethod,
  bends: usize,
  gap: f64,
) -> Result<(Vec<DVec3>, Vec<DVec3>, bool), String> {
  // closed profiles are opened at a seam so the flange can unfold: the same fraction of each,
  // the rule's gap on the shorter one (so corners at the same fraction stay matched)
  let total_a: f64 = a.lengths().iter().sum();
  let total_b: f64 = b.lengths().iter().sum();
  let seam = gap / total_a.min(total_b).max(1e-9);
  fn walk(chain: &Chain, start: f64, dir: f64, seam: f64) -> Result<Walk<'_>, String> {
    let lens = chain.lengths();
    let total: f64 = lens.iter().sum();
    let (lo, hi) = if chain.closed { (seam / 2.0 * total, total - seam / 2.0 * total) } else { (0.0, total) };
    if hi - lo < 1e-6 {
      return Err("a profile is shorter than the rule's gap".into());
    }
    Ok(Walk { chain, lens, total, start, dir, lo, hi })
  }
  let wa = walk(a, 0.0, 1.0, seam)?;
  // line the second profile up with the first: the start and direction joining them closest
  let probe: Vec<f64> = (0..=32).map(|i| i as f64 / 32.0).collect();
  let pa: Vec<DVec3> = probe.iter().map(|u| wa.at(*u)).collect();
  let starts: Vec<f64> = if b.closed { (0..72).map(|i| total_b * i as f64 / 72.0).collect() } else { vec![0.0] };
  let mut best: Option<(f64, Walk)> = None;
  for dir in [1.0, -1.0] {
    for s in &starts {
      let w = walk(b, *s, dir, seam)?;
      let cost: f64 = probe.iter().zip(&pa).map(|(u, p)| (w.at(*u) - *p).length_squared()).sum();
      if best.as_ref().is_none_or(|(c, _)| cost < *c) {
        best = Some((cost, w));
      }
    }
  }
  let wb = best.ok_or("the second profile is empty")?.1;

  // facet boundaries: the corners of both profiles, then each span subdivided
  let mut breaks: Vec<f64> = vec![0.0, 1.0];
  breaks.extend(wa.corner_us());
  breaks.extend(wb.corner_us());
  breaks.sort_by(f64::total_cmp);
  breaks.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
  let spans: Vec<(f64, f64, bool)> =
    breaks.windows(2).map(|w| (w[0], w[1], wa.straight(w[0], w[1]) && wb.straight(w[0], w[1]))).collect();
  let curved: f64 = spans.iter().filter(|s| !s.2).map(|s| s.1 - s.0).sum();
  let straight_count = spans.iter().filter(|s| s.2).count();
  let mut us = vec![0.0];
  for (u0, u1, straight) in &spans {
    let m = match method {
      LoftMethod::DieFormed => ((u1 - u0) * DIE_FACETS).ceil().max(1.0) as usize,
      LoftMethod::PressBrake if *straight => 1,
      LoftMethod::PressBrake => {
        let facets = (bends + 1).saturating_sub(straight_count).max(1) as f64;
        (facets * (u1 - u0) / curved.max(1e-12)).round().max(1.0) as usize
      }
    };
    us.extend((1..=m).map(|i| u0 + (u1 - u0) * i as f64 / m as f64));
  }
  let pa: Vec<DVec3> = us.iter().map(|u| wa.at(*u)).collect();
  let pb: Vec<DVec3> = us.iter().map(|u| wb.at(*u)).collect();
  Ok((pa, pb, breaks.len() > 2))
}

/// Two matched rows of points (the profiles) and the quads between them.
struct Strip {
  a: Vec<DVec3>,
  b: Vec<DVec3>,
  /// Unit normal of each quad, towards the material.
  n: Vec<DVec3>,
  /// Per quad: split along a[i]–b[i + 1] (else a[i + 1]–b[i]).
  diag: Vec<bool>,
  /// Per point of a and of b: the direction to offset it by along the material side, scaled so
  /// an offset by d keeps the faces next to it d apart.
  va: Vec<DVec3>,
  vb: Vec<DVec3>,
}

impl Strip {
  fn new(a: Vec<DVec3>, b: Vec<DVec3>, flip: bool, smooth: bool) -> Result<Strip, String> {
    let quads = a.len().saturating_sub(1);
    if quads == 0 {
      return Err("the profiles are too short".into());
    }
    let mut n = Vec::with_capacity(quads);
    let mut diag = Vec::with_capacity(quads);
    for i in 0..quads {
      let v = (b[i + 1] - a[i]).cross(b[i] - a[i + 1]);
      let v = v.try_normalize().ok_or("the profiles touch (a facet has no area)")?;
      n.push(if flip { -v } else { v });
      diag.push((b[i + 1] - a[i]).length() <= (a[i + 1] - b[i]).length());
    }
    let m = a.len();
    let mut st = Strip { a, b, n, diag, va: Vec::new(), vb: Vec::new() };
    if smooth {
      // the surface normal across each profile (tangent × generator)
      let normal = |row: &[DVec3], i: usize, g: DVec3| {
        let m = row.len();
        let dir = |j: usize| (row[j + 1] - row[j]).normalize_or_zero();
        let t = if m < 3 {
          dir(0)
        } else if i == 0 {
          dir(0) * 1.5 - dir(1) * 0.5
        } else if i == m - 1 {
          dir(m - 2) * 1.5 - dir(m - 3) * 0.5
        } else {
          dir(i - 1) + dir(i)
        };
        let v = t.cross(g).normalize_or_zero();
        let q = st.n[i.min(st.n.len() - 1)];
        if v.dot(q) < 0.0 { -v } else { v }
      };
      st.va = (0..m).map(|i| normal(&st.a, i, st.b[i] - st.a[i])).collect();
      st.vb = (0..m).map(|i| normal(&st.b, i, st.b[i] - st.a[i])).collect();
    } else {
      // flat facets: the triangles' normals weighted by their angles at the point (so a flat
      // quad counts once however it is split), stretched to keep each facet d away
      let pts: Vec<DVec3> = st.a.iter().chain(&st.b).copied().collect();
      let sign = if flip { -1.0 } else { 1.0 };
      let mut sum = vec![DVec3::ZERO; 2 * m];
      let mut tri_n: Vec<Vec<DVec3>> = vec![Vec::new(); 2 * m];
      for tri in st.triangles() {
        let nt = (pts[tri[1]] - pts[tri[0]]).cross(pts[tri[2]] - pts[tri[0]]).normalize_or_zero() * sign;
        for (j, &v) in tri.iter().enumerate() {
          let (p, q) = (pts[tri[(j + 1) % 3]] - pts[v], pts[tri[(j + 2) % 3]] - pts[v]);
          sum[v] += nt * p.angle_between(q);
          tri_n[v].push(nt);
        }
      }
      let vs: Vec<DVec3> = (0..2 * m)
        .map(|v| {
          let u = sum[v].normalize_or_zero();
          let c = tri_n[v].iter().map(|nt| u.dot(*nt)).fold(1.0, f64::min).max(0.5);
          u / c
        })
        .collect();
      st.va = vs[..m].to_vec();
      st.vb = vs[m..].to_vec();
    }
    Ok(st)
  }

  /// The triangles with area on the profiles (a fan's apex repeats one point).
  fn triangles(&self) -> Vec<[usize; 3]> {
    let pts: Vec<DVec3> = self.a.iter().chain(&self.b).copied().collect();
    let area = |t: &[usize; 3]| (pts[t[1]] - pts[t[0]]).cross(pts[t[2]] - pts[t[0]]).length();
    self.all_triangles().into_iter().filter(|t| area(t) > 1e-12).collect()
  }

  /// The triangles (indices into a ++ b), counter-clockwise about the unflipped normal.
  fn all_triangles(&self) -> Vec<[usize; 3]> {
    let m = self.a.len();
    let mut out = Vec::new();
    for i in 0..self.n.len() {
      let (a0, a1, b0, b1) = (i, i + 1, m + i, m + i + 1);
      if self.diag[i] {
        out.push([a0, a1, b1]);
        out.push([a0, b1, b0]);
      } else {
        out.push([a0, a1, b0]);
        out.push([a1, b1, b0]);
      }
    }
    out
  }

  /// A press-brake flange: the facets thickened along the material normal.
  fn solid(&self, t: f64) -> Result<Shape, String> {
    let m = self.a.len();
    let mut pos: Vec<DVec3> = self.a.iter().chain(&self.b).copied().collect();
    for (i, p) in self.a.iter().enumerate() {
      pos.push(*p + self.va[i] * t);
    }
    for (i, p) in self.b.iter().enumerate() {
      pos.push(*p + self.vb[i] * t);
    }
    let q = self.n.len();
    // points that coincide (a fan's apex) become one vertex
    let canon: Vec<usize> =
      (0..pos.len()).map(|i| (0..i).find(|&j| (pos[j] - pos[i]).length() < 1e-9).unwrap_or(i)).collect();
    let mut tris: Vec<u32> = Vec::new();
    let mut put = |tri: [usize; 3], out: DVec3| {
      let [p, r, s] = tri.map(|i| canon[i]);
      let w = (pos[r] - pos[p]).cross(pos[s] - pos[p]);
      if p == r || r == s || p == s || w.length() < 1e-12 {
        return;
      }
      let tri = if w.dot(out) < 0.0 { [p, s, r] } else { [p, r, s] };
      tris.extend(tri.iter().map(|&i| i as u32));
    };
    for tri in self.all_triangles() {
      // the quad a triangle belongs to: its lowest point index along a row
      let nm = self.n[(tri.iter().map(|&i| i % m).min().unwrap_or(0)).min(q - 1)];
      put(tri, -nm);
      put(tri.map(|i| i + 2 * m), nm);
    }
    // side walls along both profiles and across both ends
    let low = 2 * m;
    for i in 0..q {
      let out_a = self.a[i] - self.b[i];
      put([i, i + 1, low + i + 1], out_a);
      put([i, low + i + 1, low + i], out_a);
      let (b0, b1) = (m + i, m + i + 1);
      put([b0, b1, low + b1], -out_a);
      put([b0, low + b1, low + b0], -out_a);
    }
    for (i, inward) in [(0, self.a[1] - self.a[0]), (m - 1, self.a[m - 2] - self.a[m - 1])] {
      let (a0, b0) = (i, m + i);
      put([a0, b0, low + b0], -inward);
      put([a0, low + b0, low + a0], -inward);
    }
    let shape = Shape::from_triangles(&pos, &tris, true).map_err(|e| e.to_string())?;
    if shape.solid_count() == 0 {
      return Err("the press-brake flange did not close into a solid".into());
    }
    match shape.mass() {
      Ok(mp) if mp.volume < 0.0 => shape.reversed().map_err(|e| e.to_string()),
      _ => Ok(shape),
    }
  }

  /// A die-formed flange: smooth faces through the profiles and their offsets (all interpolated
  /// at the same parameters, so they share edges), sewn into a solid.
  fn smooth_solid(&self, t: f64) -> Result<Shape, String> {
    let m = self.a.len();
    let ke = |e: fox_kernel::KernelError| e.to_string();
    let a2: Vec<DVec3> = (0..m).map(|i| self.a[i] + self.va[i] * t).collect();
    let b2: Vec<DVec3> = (0..m).map(|i| self.b[i] + self.vb[i] * t).collect();
    // chord-length parameters, averaged over both profiles
    let mut u = vec![0.0];
    for i in 1..m {
      let step = ((self.a[i] - self.a[i - 1]).length() + (self.b[i] - self.b[i - 1]).length()) / 2.0;
      u.push(u[i - 1] + step.max(1e-9));
    }
    let faces = [
      Shape::ruled_through(&self.a, &self.b, &u).map_err(ke)?,
      Shape::ruled_through(&a2, &b2, &u).map_err(ke)?,
      Shape::ruled_through(&self.a, &a2, &u).map_err(ke)?,
      Shape::ruled_through(&self.b, &b2, &u).map_err(ke)?,
      Shape::ruled_through(&[self.a[0], self.b[0]], &[a2[0], b2[0]], &[0.0, 1.0]).map_err(ke)?,
      Shape::ruled_through(&[self.a[m - 1], self.b[m - 1]], &[a2[m - 1], b2[m - 1]], &[0.0, 1.0]).map_err(ke)?,
    ];
    let shape = Shape::compound(&faces).map_err(ke)?.sewn(1e-6).map_err(ke)?;
    if shape.solid_count() == 0 {
      return Err("the die-formed flange did not close into a solid".into());
    }
    match shape.mass() {
      Ok(mp) if mp.volume < 0.0 => shape.reversed().map_err(ke),
      _ => Ok(shape),
    }
  }

  /// The flat pattern: the neutral layer developed triangle by triangle into a plane, with a
  /// bend at every fold of a press-brake flange.
  fn develop(&self, t: f64, k: f64, folds: bool) -> Result<Development, String> {
    let m = self.a.len();
    // which way does the sheet curve? (the neutral layer is K·t from the inside of the bends)
    let turn = |i: usize| -> f64 {
      (self.a[i + 1] - self.a[i]).dot(self.n[i - 1]) + (self.b[i + 1] - self.b[i]).dot(self.n[i - 1])
    };
    let toward: f64 = (1..m - 1).map(turn).sum();
    // (never exactly on the profile: a fan's apex must spread to develop)
    let d = if toward > 0.0 { t * (1.0 - k) } else { t * k }.max(1e-4 * t);
    let pts: Vec<DVec3> =
      (0..m).map(|i| self.a[i] + self.va[i] * d).chain((0..m).map(|i| self.b[i] + self.vb[i] * d)).collect();
    // place the first two points, then each triangle's third point from the two it shares
    let mut flat: Vec<Option<P2>> = vec![None; 2 * m];
    flat[0] = Some([0.0, 0.0]);
    flat[m] = Some([(pts[m] - pts[0]).length(), 0.0]);
    let tris = self.all_triangles();
    let mut progress = true;
    while progress {
      progress = false;
      for tri in &tris {
        let unknown: Vec<usize> = (0..3).filter(|&j| flat[tri[j]].is_none()).collect();
        if unknown.len() != 1 {
          continue;
        }
        progress = true;
        // rotate so the unknown point comes last: (p, q, x) counter-clockwise
        let j = unknown[0];
        let (p, q, x) = (tri[(j + 1) % 3], tri[(j + 2) % 3], tri[j]);
        let (Some(fp), Some(fq)) = (flat[p], flat[q]) else { continue };
        let e = sub(fq, fp);
        let c = len2(e);
        if c < 1e-12 {
          return Err("a lofted flange facet has no width".into());
        }
        let (da, db) = ((pts[x] - pts[p]).length(), (pts[x] - pts[q]).length());
        let along = (da * da - db * db + c * c) / (2.0 * c);
        let h = (da * da - along * along).max(0.0).sqrt();
        let u = [e[0] / c, e[1] / c];
        flat[x] = Some([fp[0] + u[0] * along - u[1] * h, fp[1] + u[1] * along + u[0] * h]);
      }
    }
    let flat: Vec<P2> =
      flat.into_iter().collect::<Option<Vec<_>>>().ok_or("the lofted flange could not be developed")?;
    // 3D frame of the development: the first point, first edge along x, the facet normal
    // (counter-clockwise) along z, mirrored so the plane faces away from the material
    let ex = (pts[m] - pts[0]).try_normalize().ok_or("the profiles touch")?;
    let n0 = self.n[0];
    let unflipped = (self.b[1] - self.a[0]).cross(self.b[0] - self.a[1]);
    let ey = unflipped.normalize_or_zero().cross(ex);
    let toward_material = unflipped.dot(n0) > 0.0;
    let (plane, ysign) = if toward_material {
      (Plane { origin: pts[0], x: ex, y: -ey }, -1.0)
    } else {
      (Plane { origin: pts[0], x: ex, y: ey }, 1.0)
    };
    let to2 = |p: P2| [p[0], p[1] * ysign];
    let mut outline: Vec<P2> = (0..m).map(|i| to2(flat[i])).collect();
    outline.extend((0..m).rev().map(|i| to2(flat[m + i])));
    let mut bends = Vec::new();
    if folds {
      // folds between neighbouring facets (along a diagonal or a generator), on the profiles
      let prof: Vec<DVec3> = self.a.iter().chain(&self.b).copied().collect();
      let normal =
        |tri: &[usize; 3]| (prof[tri[1]] - prof[tri[0]]).cross(prof[tri[2]] - prof[tri[0]]).normalize_or_zero();
      let ftris = self.triangles();
      let near = |i: usize, j: usize| (prof[i] - prof[j]).length() < 1e-9;
      for j in 0..ftris.len() {
        for l in j + 1..(j + 4).min(ftris.len()) {
          let (t1, t2) = (ftris[j], ftris[l]);
          let shared: Vec<usize> = t1.iter().copied().filter(|&v| t2.iter().any(|&w| near(v, w))).collect();
          if shared.len() != 2 {
            continue;
          }
          let (n1, n2) = (normal(&t1), normal(&t2));
          let angle = n1.dot(n2).clamp(-1.0, 1.0).acos();
          if angle < 1e-3 {
            continue;
          }
          let far = t2.iter().copied().find(|&w| !shared.iter().any(|&v| near(v, w))).unwrap_or(shared[0]);
          let mat = if toward_material { n1 } else { -n1 };
          let inner = (prof[far] - prof[shared[0]]).dot(mat) < 0.0;
          bends.push(Bend {
            start: plane.to_world(to2(flat[shared[0]])),
            end: plane.to_world(to2(flat[shared[1]])),
            angle,
            inner,
          });
        }
      }
    }
    Ok(Development { plane, outline, bends, volume: 0.0 })
  }
}

// ---------------------------------------------------------------- miter flange

/// Maps a segment through a rigid 2D map (`mirror`: the map flips orientation).
pub fn map_seg(s: &Seg, f: &impl Fn(P2) -> P2, mirror: bool) -> Seg {
  let sg = if mirror { -1.0 } else { 1.0 };
  match *s {
    Seg::Line { a, b } => Seg::Line { a: f(a), b: f(b) },
    Seg::Arc { center, radius, sweep, .. } => {
      let c = f(center);
      let d = sub(f(s.start()), c);
      Seg::Arc { center: c, radius, start_angle: d[1].atan2(d[0]), sweep: sweep * sg }
    }
    Seg::Circle { center, radius, ccw } => Seg::Circle { center: f(center), radius, ccw: ccw != mirror },
    Seg::Bezier { p } => Seg::Bezier { p: p.map(f) },
  }
}

/// The section of a miter flange from its profile path, in the flange plane of an edge (x out
/// of the sheet edge, y the sheet face's normal, see [`sheet::flange_section`]): the path starts
/// on the edge, is one face of the flange, and the sheet runs into it from −x. Returns the
/// section (its first bend starting at x = 0), the angle of that bend and how far before the
/// edge it starts (the sheet is trimmed back that far so the flange lands where it was drawn).
pub fn miter_section(path: &[Seg], t: f64, r: f64) -> Result<(Profile, f64, f64), String> {
  let first = path.first().ok_or("the miter flange profile is empty")?;
  let o = first.start();
  let path: Vec<Seg> = path.iter().map(|s| map_seg(s, &|p| sub(p, o), false)).collect();
  // the first direction decides the first bend
  let p1 = seg_at(&path[0], 1e-4);
  let turn = p1[1].atan2(p1[0]);
  if turn.abs() > PI * 179.0 / 180.0 {
    return Err("the miter flange profile folds back onto the sheet".into());
  }
  // the sheet's last stretch leads into the profile and the corner becomes the first bend
  let rho = if turn < 0.0 { r + t } else { r };
  let cut = rho * (turn.abs() / 2.0).tan();
  let lead = Seg::Line { a: [-(cut + t), 0.0], b: [0.0, 0.0] };
  let mut full = vec![lead];
  full.extend(path);
  let prof = sheet::contour_section(&full, t, r, true)?;
  // drop the lead-in (it lies in the sheet) and close the section where the bend starts
  let segs = &prof.outer.segs;
  let behind = |s: &Seg| s.start()[0] < -cut + 1e-9 && s.end()[0] < -cut + 1e-9 && s.mid()[0] < -cut + 1e-9;
  let first = (0..segs.len()).find(|&i| !behind(&segs[i]) && behind(&segs[(i + segs.len() - 1) % segs.len()]));
  let first = first.ok_or("the miter flange profile has no bend")?;
  let kept: Vec<Seg> = (0..segs.len()).map(|i| segs[(first + i) % segs.len()]).filter(|s| !behind(s)).collect();
  let (Some(a), Some(b)) = (kept.last().map(Seg::end), kept.first().map(Seg::start)) else {
    return Err("the miter flange profile is empty".into());
  };
  let mut kept = kept;
  kept.push(Seg::Line { a, b });
  let shift = |p: P2| [p[0] + cut, p[1]];
  let outer = Loop { segs: kept.iter().map(|s| map_seg(s, &shift, false)).collect() };
  Ok((Profile { outer, holes: Vec::new() }, turn.abs(), cut))
}

/// How far a flange section reaches out (max x) and where its wall starts (the least x of the
/// section above the bend, `bend_top`).
pub fn section_extents(p: &Profile, bend_top: f64) -> (f64, f64) {
  let pts = p.outer.polygon();
  let xout = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
  let xin = pts.iter().filter(|q| q[1] > bend_top - 1e-9).map(|q| q[0]).fold(f64::INFINITY, f64::min);
  (xout, if xin.is_finite() { xin } else { xout })
}

/// Height of a bend of `angle` (inside radius r, thickness t) above the sheet face it starts
/// from (the top of the region a corner relief clears).
pub fn bend_top(r: f64, t: f64, angle: f64) -> f64 {
  if angle <= PI / 2.0 { r * (1.0 - angle.cos()) } else { r + (r + t) * (angle - PI / 2.0).sin().max(0.0) }
}

// ---------------------------------------------------------------- corner reliefs

/// The material a corner relief removes where two flanges meet: a prism through the sheet and
/// up to `top` (along `up`), centred on the corner point of the sheet face. `wi` and `wj` point
/// out of the two edges.
pub fn corner_relief(
  corner: DVec3,
  wi: DVec3,
  wj: DVec3,
  up: DVec3,
  top: f64,
  sheet: &SheetInfo,
  relief: CornerRelief,
) -> Result<Option<Shape>, String> {
  let s = sheet.corner_relief_size();
  let t = sheet.thickness;
  let x = wi.try_normalize().ok_or("bad corner")?;
  let y = (wj - x * wj.dot(x)).try_normalize().ok_or("the flanges at a corner are parallel")?;
  let pl = Plane { origin: corner - up * (2.0 * t), x, y };
  let prof = match relief {
    CornerRelief::None => return Ok(None),
    CornerRelief::Square => sheet::rectangle([-s / 2.0, -s / 2.0], [s / 2.0, s / 2.0]),
    CornerRelief::Round | CornerRelief::Tear => Profile {
      outer: Loop { segs: vec![Seg::Circle { center: [0.0, 0.0], radius: s / 2.0, ccw: true }] },
      holes: Vec::new(),
    },
  };
  let face = Shape::face(&pl, &prof).map_err(|e| e.to_string())?;
  Ok(Some(face.extrude(up * (top + 2.0 * t + 1e-3)).map_err(|e| e.to_string())?))
}

// ---------------------------------------------------------------- flat pattern sketch

/// The flat pattern as an editable sketch on `plane` (which should be the flat's plane or
/// parallel to it): the outline's lines, arcs and circles (other curves as polylines) and the
/// bend centre lines as construction lines. `shift` moves everything in the sketch.
pub fn flat_sketch(flat: &FlatPattern, plane: &Plane, shift: P2) -> Result<Sketch, String> {
  use fox_sketch::{CurveKind, PointId};
  let pl = &flat.plane;
  let n = pl.normal();
  let on_plane = |p: DVec3| (p - pl.origin).dot(n).abs() < 1e-4;
  let edges = flat.shape.edges().map_err(|e| e.to_string())?;
  let lines = flat.shape.edge_lines(0.01).map_err(|e| e.to_string())?;
  let loc = |v: DVec3| {
    let q = plane.to_local(v);
    [q[0] + shift[0], q[1] + shift[1]]
  };
  let mut sk = Sketch::new();
  // ends shared by outline curves become shared sketch points
  let mut points: Vec<(P2, PointId)> = Vec::new();
  let mut pt = |sk: &mut Sketch, q: P2| -> PointId {
    if let Some((_, id)) = points.iter().find(|(p, _)| len2(sub(*p, q)) < 1e-6) {
      return *id;
    }
    let id = sk.add_point(q[0], q[1]);
    points.push((q, id));
    id
  };
  let mut kinds = Vec::new();
  for e in &edges {
    if !(on_plane(e.start) && on_plane(e.end) && on_plane(e.mid)) || e.kind == fox_kernel::CurveKind::Degenerate {
      continue;
    }
    match e.kind {
      fox_kernel::CurveKind::Line => {
        kinds.push(CurveKind::Line { a: pt(&mut sk, loc(e.start)), b: pt(&mut sk, loc(e.end)) })
      }
      fox_kernel::CurveKind::Circle if (e.start - e.end).length() < 1e-6 => {
        kinds.push(CurveKind::Circle { center: pt(&mut sk, loc(e.center)), radius: e.radius })
      }
      fox_kernel::CurveKind::Circle => {
        // sketch arcs run counter-clockwise (seen along the sketch normal) from start to end
        let (a, b) = if e.axis.dot(plane.normal()) > 0.0 { (e.start, e.end) } else { (e.end, e.start) };
        kinds.push(CurveKind::Arc {
          center: pt(&mut sk, loc(e.center)),
          start: pt(&mut sk, loc(a)),
          end: pt(&mut sk, loc(b)),
        });
      }
      _ => {
        for l in lines.iter().filter(|l| l.id == e.id) {
          for w in l.points.windows(2) {
            if len2(sub(loc(w[0]), loc(w[1]))) > 1e-9 {
              kinds.push(CurveKind::Line { a: pt(&mut sk, loc(w[0])), b: pt(&mut sk, loc(w[1])) });
            }
          }
        }
      }
    }
  }
  if kinds.is_empty() {
    return Err("the flat pattern has no outline".into());
  }
  let err = |e: fox_sketch::SketchError| e.to_string();
  for k in kinds {
    sk.add_curve(k).map_err(err)?;
  }
  for b in &flat.bends {
    let (s, e) = (loc(b.start), loc(b.end));
    let (id, _) = sk.add_line(PtSpec::At(s[0], s[1]), PtSpec::At(e[0], e[1])).map_err(err)?;
    sk.set_construction(id, true).map_err(err)?;
  }
  Ok(sk)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn miter_section_of_a_right_angle_flange() {
    // straight up 20 from the edge: the same section as a 90° edge flange
    let (t, r) = (2.0, 3.0);
    let (p, turn, cut) = miter_section(&[Seg::Line { a: [0.0, 0.0], b: [0.0, 20.0] }], t, r).unwrap();
    assert!((turn - PI / 2.0).abs() < 1e-9 && (cut - r).abs() < 1e-9);
    let flange = sheet::flange_section(r, t, PI / 2.0, 20.0 - r);
    assert!((p.area() - flange.area()).abs() < 0.05, "{} {}", p.area(), flange.area());
    let (xout, xin) = section_extents(&p, bend_top(r, t, turn));
    assert!((xout - (r + t)).abs() < 1e-9 && (xin - r).abs() < 1e-9, "{xout} {xin}");
  }

  #[test]
  fn bend_tops() {
    assert!((bend_top(3.0, 2.0, PI / 2.0) - 3.0).abs() < 1e-12);
    assert!((bend_top(3.0, 2.0, PI) - 8.0).abs() < 1e-12);
  }
}
