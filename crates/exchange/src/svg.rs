//! SVG: 2D geometry in and out of sketches, as the same [`Entity`] list DXF uses.
//!
//! Reading handles `<path>` (every command, relative and absolute), `<rect>` (with rounded
//! corners), `<circle>`, `<ellipse>`, `<line>`, `<polyline>` and `<polygon>`, nested `<g>` and
//! `transform`s, and the root `viewBox`/`width`/`height` (px at 96 dpi, mm, cm, in, pt, pc).
//! User units become millimetres, keeping their place: the y axis is mirrored about the middle
//! of the viewBox, so a page whose viewBox starts at 0,0 lands with its bottom-left corner on
//! the origin, y up. Quadratic Béziers become cubics; elliptical arcs become arcs when they are
//! circular after the transform, cubics otherwise. `<text>`, `<image>`, `<use>` and hidden
//! elements are counted in [`Drawing::skipped`]; `<defs>`, `<clipPath>`, `<mask>`, `<symbol>`
//! and other non-drawing content is ignored. Elements in a group whose id, class or label
//! mentions "construction" are construction geometry (how FissionOxide writes it).
//!
//! Writing produces a standalone SVG in millimetres, stroke only, with connected entities joined
//! into one `<path>` each.

use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::fmt::Write as _;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::dxf::{Drawing, Entity};
use crate::{ExchangeError, Result};

type P = [f64; 2];

/// Millimetres per CSS pixel (96 dpi).
const PX: f64 = 25.4 / 96.0;
/// A closed subpath ending this close (mm) to its start is snapped shut.
const CLOSE_SNAP: f64 = 1e-3;
/// Relative tolerance for "this transformed ellipse is a circle".
const CIRCULAR: f64 = 1e-9;

/// A 2D affine transform `[a b c d e f]`: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Xf([f64; 6]);

impl Xf {
  const ID: Xf = Xf([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

  fn translate(x: f64, y: f64) -> Xf {
    Xf([1.0, 0.0, 0.0, 1.0, x, y])
  }

  /// `self` after `o` (o applies first).
  fn mul(self, o: Xf) -> Xf {
    let [a, b, c, d, e, f] = self.0;
    let [a2, b2, c2, d2, e2, f2] = o.0;
    Xf([a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f])
  }

  fn apply(&self, p: P) -> P {
    let [a, b, c, d, e, f] = self.0;
    [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
  }

  fn vec(&self, v: P) -> P {
    let [a, b, c, d, ..] = self.0;
    [a * v[0] + c * v[1], b * v[0] + d * v[1]]
  }

  fn det(&self) -> f64 {
    let [a, b, c, d, ..] = self.0;
    a * d - b * c
  }
}

fn dist(a: P, b: P) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

// ---------------------------------------------------------------------------------------------
// numbers

/// A cursor over SVG number lists: compact syntax (`1.5.5`, `-1-2`, `1e-3`), commas or spaces.
struct Lex<'a> {
  s: &'a [u8],
  i: usize,
}

impl<'a> Lex<'a> {
  fn new(s: &'a str) -> Self {
    Lex { s: s.as_bytes(), i: 0 }
  }

  fn peek(&self) -> Option<u8> {
    self.s.get(self.i).copied()
  }

  fn skip_sep(&mut self) {
    while self.peek().is_some_and(|c| c.is_ascii_whitespace() || c == b',') {
      self.i += 1;
    }
  }

  fn at_end(&mut self) -> bool {
    self.skip_sep();
    self.i >= self.s.len()
  }

  fn digits(&mut self) -> bool {
    let start = self.i;
    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
      self.i += 1;
    }
    self.i > start
  }

  fn number(&mut self) -> Option<f64> {
    self.skip_sep();
    let start = self.i;
    if matches!(self.peek(), Some(b'+' | b'-')) {
      self.i += 1;
    }
    let mut any = self.digits();
    if self.peek() == Some(b'.') {
      self.i += 1;
      any |= self.digits();
    }
    if !any {
      self.i = start;
      return None;
    }
    if matches!(self.peek(), Some(b'e' | b'E')) {
      let mark = self.i;
      self.i += 1;
      if matches!(self.peek(), Some(b'+' | b'-')) {
        self.i += 1;
      }
      if !self.digits() {
        self.i = mark;
      }
    }
    std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok().filter(|v: &f64| v.is_finite())
  }

  /// An arc flag: a single `0` or `1`, which needs no separator after it.
  fn flag(&mut self) -> Option<bool> {
    self.skip_sep();
    let f = match self.peek()? {
      b'0' => false,
      b'1' => true,
      _ => return None,
    };
    self.i += 1;
    Some(f)
  }

  fn point(&mut self) -> Option<P> {
    Some([self.number()?, self.number()?])
  }

  fn command(&mut self) -> Option<u8> {
    self.skip_sep();
    let c = self.peek().filter(|c| c.is_ascii_alphabetic())?;
    self.i += 1;
    Some(c)
  }
}

/// Every number in a list (stops at the first thing that isn't one).
fn numbers(s: &str) -> Vec<f64> {
  let mut lx = Lex::new(s);
  std::iter::from_fn(|| lx.number()).collect()
}

/// A length in millimetres; `None` for percentages, font-relative units and junk.
fn length_mm(s: &str) -> Option<f64> {
  let s = s.trim();
  let mut lx = Lex::new(s);
  let v = lx.number()?;
  let k = match s.get(lx.i..)?.trim().to_ascii_lowercase().as_str() {
    "" | "px" => PX,
    "mm" => 1.0,
    "cm" => 10.0,
    "in" => 25.4,
    "pt" => 25.4 / 72.0,
    "pc" => 25.4 / 6.0,
    "q" => 0.25,
    _ => return None,
  };
  Some(v * k)
}

/// A shape attribute in user units (a unit on it converts as if one user unit were one px).
fn user_len(a: &Attrs, key: &str) -> Option<f64> {
  let s = a.get(key)?.trim();
  match s.parse::<f64>() {
    Ok(v) => Some(v).filter(|v| v.is_finite()),
    Err(_) => length_mm(s).map(|mm| mm / PX),
  }
}

/// A `transform` attribute; `None` when it doesn't parse (the attribute is then ignored).
fn parse_transform(s: &str) -> Option<Xf> {
  let mut m = Xf::ID;
  let mut rest = s;
  loop {
    rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    if rest.is_empty() {
      return Some(m);
    }
    let open = rest.find('(')?;
    let close = open + rest[open..].find(')')?;
    let name = rest[..open].trim();
    let args = numbers(&rest[open + 1..close]);
    rest = &rest[close + 1..];
    let rot = |deg: f64| {
      let (s, c) = deg.to_radians().sin_cos();
      Xf([c, s, -s, c, 0.0, 0.0])
    };
    let t = match (name, args.as_slice()) {
      ("matrix", &[a, b, c, d, e, f]) => Xf([a, b, c, d, e, f]),
      ("translate", &[x]) => Xf::translate(x, 0.0),
      ("translate", &[x, y]) => Xf::translate(x, y),
      ("scale", &[k]) => Xf([k, 0.0, 0.0, k, 0.0, 0.0]),
      ("scale", &[x, y]) => Xf([x, 0.0, 0.0, y, 0.0, 0.0]),
      ("rotate", &[a]) => rot(a),
      ("rotate", &[a, cx, cy]) => Xf::translate(cx, cy).mul(rot(a)).mul(Xf::translate(-cx, -cy)),
      ("skewX", &[a]) => Xf([1.0, 0.0, a.to_radians().tan(), 1.0, 0.0, 0.0]),
      ("skewY", &[a]) => Xf([1.0, a.to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
      _ => return None,
    };
    m = m.mul(t);
  }
}

// ---------------------------------------------------------------------------------------------
// geometry in user units

/// A path segment in user units, before the transform.
#[derive(Debug, Clone, Copy)]
enum Seg {
  Line(P, P),
  Cubic([P; 4]),
  /// SVG endpoint parameterisation; `phi` in degrees.
  Arc {
    from: P,
    to: P,
    rx: f64,
    ry: f64,
    phi: f64,
    large: bool,
    sweep: bool,
  },
}

/// One subpath: connected segments, closed by `Z`.
#[derive(Debug, Default)]
struct Sub {
  segs: Vec<Seg>,
  closed: bool,
}

/// Parses path data. Like a browser, it keeps everything before an error; the flag says
/// whether there was one.
fn parse_path(d: &str) -> (Vec<Sub>, bool) {
  let mut lx = Lex::new(d);
  let mut subs: Vec<Sub> = Vec::new();
  let mut segs: Vec<Seg> = Vec::new();
  let (mut cur, mut start) = ([0.0, 0.0], [0.0, 0.0]);
  let mut prev: Option<u8> = None;
  // the last cubic's second control point / the last quadratic's control point
  let (mut last_c2, mut last_q): (Option<P>, Option<P>) = (None, None);
  let flush = |segs: &mut Vec<Seg>, subs: &mut Vec<Sub>, closed: bool| {
    if !segs.is_empty() {
      subs.push(Sub { segs: std::mem::take(segs), closed });
    }
  };
  let ok = loop {
    if lx.at_end() {
      break true;
    }
    let cmd = match lx.command() {
      Some(c) => c,
      // repeated coordinates repeat the command (after a moveto, as a lineto)
      None => match prev {
        Some(b'M') => b'L',
        Some(b'm') => b'l',
        Some(b'Z' | b'z') | None => break false,
        Some(c) => c,
      },
    };
    if prev.is_none() && !matches!(cmd, b'M' | b'm') {
      break false;
    }
    let base = if cmd.is_ascii_lowercase() { cur } else { [0.0, 0.0] };
    let rel = |p: P| [p[0] + base[0], p[1] + base[1]];
    let (mut c2, mut q) = (None, None);
    let done = match cmd.to_ascii_uppercase() {
      b'M' => lx.point().map(|p| {
        flush(&mut segs, &mut subs, false);
        cur = rel(p);
        start = cur;
      }),
      b'Z' => {
        if cur != start {
          segs.push(Seg::Line(cur, start));
        }
        flush(&mut segs, &mut subs, true);
        cur = start;
        Some(())
      }
      b'L' => lx.point().map(|p| {
        let p = rel(p);
        segs.push(Seg::Line(cur, p));
        cur = p;
      }),
      b'H' => lx.number().map(|x| {
        let p = [x + base[0], cur[1]];
        segs.push(Seg::Line(cur, p));
        cur = p;
      }),
      b'V' => lx.number().map(|y| {
        let p = [cur[0], y + base[1]];
        segs.push(Seg::Line(cur, p));
        cur = p;
      }),
      b'C' | b'S' => {
        let a = if cmd.eq_ignore_ascii_case(&b'C') {
          lx.point().map(rel)
        } else {
          Some(last_c2.map_or(cur, |c| [2.0 * cur[0] - c[0], 2.0 * cur[1] - c[1]]))
        };
        match (a, lx.point(), lx.point()) {
          (Some(a), Some(b), Some(p)) => {
            let (b, p) = (rel(b), rel(p));
            segs.push(Seg::Cubic([cur, a, b, p]));
            c2 = Some(b);
            cur = p;
            Some(())
          }
          _ => None,
        }
      }
      b'Q' | b'T' => {
        let a = if cmd.eq_ignore_ascii_case(&b'Q') {
          lx.point().map(rel)
        } else {
          Some(last_q.map_or(cur, |c| [2.0 * cur[0] - c[0], 2.0 * cur[1] - c[1]]))
        };
        match (a, lx.point()) {
          (Some(a), Some(p)) => {
            let p = rel(p);
            // exact degree elevation
            let k = 2.0 / 3.0;
            let c1 = [cur[0] + k * (a[0] - cur[0]), cur[1] + k * (a[1] - cur[1])];
            let c2b = [p[0] + k * (a[0] - p[0]), p[1] + k * (a[1] - p[1])];
            segs.push(Seg::Cubic([cur, c1, c2b, p]));
            q = Some(a);
            cur = p;
            Some(())
          }
          _ => None,
        }
      }
      b'A' => {
        let parsed = (|| Some((lx.number()?, lx.number()?, lx.number()?, lx.flag()?, lx.flag()?, lx.point()?)))();
        parsed.map(|(rx, ry, phi, large, sweep, p)| {
          let p = rel(p);
          segs.push(Seg::Arc { from: cur, to: p, rx, ry, phi, large, sweep });
          cur = p;
        })
      }
      _ => None,
    };
    if done.is_none() {
      break false;
    }
    (last_c2, last_q) = (c2, q);
    prev = Some(cmd);
  };
  flush(&mut segs, &mut subs, false);
  (subs, !ok)
}

// ---------------------------------------------------------------------------------------------
// geometry in millimetres

/// A segment after the transform, with exact end points.
#[derive(Debug, Clone, Copy)]
enum TSeg {
  Line(P, P),
  Cubic([P; 4]),
  /// A circular arc from `from` to `to`, counter-clockwise or not.
  Arc {
    center: P,
    radius: f64,
    from: P,
    to: P,
    ccw: bool,
  },
}

impl TSeg {
  fn start(&self) -> P {
    match *self {
      TSeg::Line(a, _) => a,
      TSeg::Cubic(p) => p[0],
      TSeg::Arc { from, .. } => from,
    }
  }

  fn end(&self) -> P {
    match *self {
      TSeg::Line(_, b) => b,
      TSeg::Cubic(p) => p[3],
      TSeg::Arc { to, .. } => to,
    }
  }

  fn set_end(&mut self, q: P) {
    match self {
      TSeg::Line(_, b) => *b = q,
      TSeg::Cubic(p) => p[3] = q,
      TSeg::Arc { to, .. } => *to = q,
    }
  }

  fn entity(self) -> Option<Entity> {
    match self {
      TSeg::Line(a, b) => (dist(a, b) > 1e-12).then_some(Entity::Line { a, b }),
      TSeg::Cubic(p) => (p.iter().any(|q| dist(*q, p[0]) > 1e-12)).then_some(Entity::Bezier { p }),
      TSeg::Arc { center, radius, from, to, ccw } => {
        let ang = |p: P| (p[1] - center[1]).atan2(p[0] - center[0]);
        let (s, e) = if ccw { (from, to) } else { (to, from) };
        (dist(from, to) > 1e-12).then_some(Entity::Arc { center, radius, start: ang(s), end: ang(e) })
      }
    }
  }
}

/// The ellipse `center + R(phi) diag(rx, ry)` after `m`: its image is a circle when the
/// columns of the linear part are orthogonal and equally long; returns that radius.
fn circular_radius(m: &Xf, rx: f64, ry: f64, phi: f64) -> Option<f64> {
  let (s, c) = phi.sin_cos();
  let a1 = m.vec([rx * c, rx * s]);
  let a2 = m.vec([-ry * s, ry * c]);
  let (l1, l2) = (a1[0].hypot(a1[1]), a2[0].hypot(a2[1]));
  let dot = a1[0] * a2[0] + a1[1] * a2[1];
  let scale = l1.max(l2);
  (scale > 0.0 && (l1 - l2).abs() <= CIRCULAR * scale && dot.abs() <= CIRCULAR * scale * scale)
    .then_some((l1 + l2) / 2.0)
}

/// Cubic pieces (at most 90° each) of the ellipse arc `center + R(phi) diag(rx, ry) (cos t, sin t)`
/// for t from `t0` by `dt`, transformed by `m`.
fn ellipse_cubics(m: &Xf, center: P, rx: f64, ry: f64, phi: f64, t0: f64, dt: f64) -> Vec<[P; 4]> {
  let n = ((dt.abs() / FRAC_PI_2) - 1e-9).ceil().max(1.0) as usize;
  let step = dt / n as f64;
  let k = 4.0 / 3.0 * (step / 4.0).tan();
  let (sp, cp) = phi.sin_cos();
  let on = |t: f64| {
    let (s, c) = t.sin_cos();
    let (x, y) = (rx * c, ry * s);
    [center[0] + cp * x - sp * y, center[1] + sp * x + cp * y]
  };
  let tangent = |t: f64| {
    let (s, c) = t.sin_cos();
    let (x, y) = (-rx * s, ry * c);
    [cp * x - sp * y, sp * x + cp * y]
  };
  (0..n)
    .map(|i| {
      let (ta, tb) = (t0 + step * i as f64, t0 + step * (i + 1) as f64);
      let (pa, pb, da, db) = (on(ta), on(tb), tangent(ta), tangent(tb));
      [pa, [pa[0] + k * da[0], pa[1] + k * da[1]], [pb[0] - k * db[0], pb[1] - k * db[1]], pb].map(|p| m.apply(p))
    })
    .collect()
}

/// An SVG arc segment after `m`: one circular arc, cubics, a line (zero radius) or nothing.
fn arc_segs(m: &Xf, from: P, to: P, rx: f64, ry: f64, phi_deg: f64, large: bool, sweep: bool, out: &mut Vec<TSeg>) {
  let (fa, ta) = (m.apply(from), m.apply(to));
  if from == to {
    return;
  }
  let (mut rx, mut ry) = (rx.abs(), ry.abs());
  if rx == 0.0 || ry == 0.0 {
    out.push(TSeg::Line(fa, ta));
    return;
  }
  // endpoint to centre parameterisation (SVG 1.1 appendix F.6.5)
  let phi = phi_deg.to_radians();
  let (s, c) = phi.sin_cos();
  let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
  let (x1, y1) = (c * dx + s * dy, -s * dx + c * dy);
  let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
  if lambda > 1.0 {
    rx *= lambda.sqrt();
    ry *= lambda.sqrt();
  }
  let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
  let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
  let mut k = if den > 0.0 { (num / den).max(0.0).sqrt() } else { 0.0 };
  if large == sweep {
    k = -k;
  }
  let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
  let center = [c * cx1 - s * cy1 + (from[0] + to[0]) / 2.0, s * cx1 + c * cy1 + (from[1] + to[1]) / 2.0];
  let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
  let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
  let t0 = uy.atan2(ux);
  let mut dt = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
  if !sweep && dt > 0.0 {
    dt -= TAU;
  } else if sweep && dt < 0.0 {
    dt += TAU;
  }
  if let Some(radius) = circular_radius(m, rx, ry, phi) {
    // a mirroring transform reverses the direction
    let ccw = (dt > 0.0) == (m.det() > 0.0);
    out.push(TSeg::Arc { center: m.apply(center), radius, from: fa, to: ta, ccw });
    return;
  }
  let mut cubics = ellipse_cubics(m, center, rx, ry, phi, t0, dt);
  if let Some(first) = cubics.first_mut() {
    first[0] = fa;
  }
  if let Some(last) = cubics.last_mut() {
    last[3] = ta;
  }
  out.extend(cubics.into_iter().map(TSeg::Cubic));
}

/// Transforms subpaths into entities; closed subpaths end exactly where they start.
fn emit_subs(subs: &[Sub], m: &Xf, cons: bool, out: &mut Vec<(Entity, bool)>) {
  for sub in subs {
    let mut t: Vec<TSeg> = Vec::new();
    for seg in &sub.segs {
      match *seg {
        Seg::Line(a, b) => {
          if a != b {
            t.push(TSeg::Line(m.apply(a), m.apply(b)));
          }
        }
        Seg::Cubic(p) => {
          if p.iter().any(|q| *q != p[0]) {
            t.push(TSeg::Cubic(p.map(|q| m.apply(q))));
          }
        }
        Seg::Arc { from, to, rx, ry, phi, large, sweep } => arc_segs(m, from, to, rx, ry, phi, large, sweep, &mut t),
      }
    }
    if sub.closed && t.len() > 1 {
      let start = t[0].start();
      // a sliver closing line (rounding in relative coordinates) folds into the segment before
      if let Some(TSeg::Line(a, b)) = t.last()
        && dist(*a, *b) < CLOSE_SNAP
      {
        t.pop();
      }
      if let Some(last) = t.last_mut() {
        if dist(last.end(), start) < CLOSE_SNAP {
          last.set_end(start);
        } else {
          let e = last.end();
          t.push(TSeg::Line(e, start));
        }
      }
    }
    out.extend(t.into_iter().filter_map(TSeg::entity).map(|e| (e, cons)));
  }
}

/// A circle or ellipse: a circle entity when it stays circular, else four cubics.
fn emit_ellipse(m: &Xf, center: P, rx: f64, ry: f64, cons: bool, out: &mut Vec<(Entity, bool)>) {
  if !(rx > 0.0 && ry > 0.0) {
    return;
  }
  if let Some(radius) = circular_radius(m, rx, ry, 0.0) {
    out.push((Entity::Circle { center: m.apply(center), radius }, cons));
    return;
  }
  let mut cubics = ellipse_cubics(m, center, rx, ry, 0.0, 0.0, TAU);
  let first = cubics.first().map(|c| c[0]);
  if let (Some(f), Some(last)) = (first, cubics.last_mut()) {
    last[3] = f;
  }
  out.extend(cubics.into_iter().map(|p| (Entity::Bezier { p }, cons)));
}

// ---------------------------------------------------------------------------------------------
// the document

type Attrs = HashMap<String, String>;

/// Unprefixed attributes by name, plus a prefixed `label` (Inkscape's layer names).
fn attrs(e: &BytesStart<'_>) -> Attrs {
  let mut out = Attrs::new();
  for a in e.attributes().with_checks(false).flatten() {
    let local = a.key.local_name().as_ref().to_string();
    let value =
      a.normalized_value(XmlVersion::Implicit1_0).map(|v| v.into_owned()).unwrap_or_else(|_| a.value.to_string());
    if a.key.prefix().is_none() {
      out.insert(local, value);
    } else if local == "label" {
      out.entry(local).or_insert(value);
    }
  }
  out
}

/// A presentation property from the attribute or the `style` attribute.
fn property(a: &Attrs, name: &str) -> Option<String> {
  let from_style = a.get("style").and_then(|s| {
    s.split(';').filter_map(|d| d.split_once(':')).find(|(k, _)| k.trim() == name).map(|(_, v)| v.trim().to_string())
  });
  from_style.or_else(|| a.get(name).map(|v| v.trim().to_string()))
}

/// Maps the root's user units to millimetres, y up, mirrored about the viewBox's middle.
fn root_transform(a: &Attrs) -> Xf {
  let w = a.get("width").and_then(|v| length_mm(v)).filter(|v| *v > 0.0);
  let h = a.get("height").and_then(|v| length_mm(v)).filter(|v| *v > 0.0);
  let vb = a.get("viewBox").map(|v| numbers(v)).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
  match vb {
    Some(vb) => {
      let stretch = a.get("preserveAspectRatio").is_some_and(|p| p.trim().starts_with("none"));
      let (sx, sy) = match (w.map(|w| w / vb[2]), h.map(|h| h / vb[3])) {
        (Some(x), Some(y)) if stretch => (x, y),
        (Some(x), Some(y)) => (x.min(y), x.min(y)),
        (Some(k), None) | (None, Some(k)) => (k, k),
        (None, None) => (PX, PX),
      };
      Xf([sx, 0.0, 0.0, -sy, 0.0, sy * (2.0 * vb[1] + vb[3])])
    }
    None => Xf([PX, 0.0, 0.0, -PX, 0.0, h.unwrap_or(0.0)]),
  }
}

#[derive(Clone, Copy)]
struct Frame {
  xf: Xf,
  construction: bool,
  skip: bool,
}

fn skip_count(d: &mut Drawing, what: &str) {
  *d.skipped.entry(what.to_string()).or_insert(0) += 1;
}

/// Handles one element; returns the frame its children use.
fn element(d: &mut Drawing, parent: Frame, name: &str, a: &Attrs) -> Frame {
  let skip = Frame { skip: true, ..parent };
  if parent.skip {
    return skip;
  }
  let hidden = property(a, "display").is_some_and(|v| v == "none");
  let is_shape = matches!(name, "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon");
  if hidden && (is_shape || matches!(name, "g" | "svg" | "a" | "switch")) {
    skip_count(d, "hidden element");
    return skip;
  }
  let own = a.get("transform").and_then(|t| parse_transform(t)).unwrap_or(Xf::ID);
  let mut xf = parent.xf.mul(own);
  let tag =
    ["id", "class", "label"].iter().any(|k| a.get(*k).is_some_and(|v| v.to_ascii_lowercase().contains("construction")));
  let cons = parent.construction || tag;
  let num = |k: &str| user_len(a, k).unwrap_or(0.0);
  let out = &mut d.entities;
  match name {
    "g" | "a" | "switch" => {}
    "svg" => {
      // a nested viewport: its position only (its own viewBox isn't applied)
      xf = parent.xf.mul(Xf::translate(num("x"), num("y"))).mul(own);
    }
    "path" => {
      let (subs, error) = parse_path(a.get("d").map_or("", String::as_str));
      emit_subs(&subs, &xf, cons, out);
      if error {
        skip_count(d, "path data after an error");
      }
    }
    "rect" => {
      let (x, y, w, h) = (num("x"), num("y"), num("width"), num("height"));
      if w > 0.0 && h > 0.0 {
        let (rx, ry) = match (user_len(a, "rx").filter(|v| *v >= 0.0), user_len(a, "ry").filter(|v| *v >= 0.0)) {
          (Some(rx), Some(ry)) => (rx, ry),
          (Some(r), None) | (None, Some(r)) => (r, r),
          (None, None) => (0.0, 0.0),
        };
        let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
        let mut segs = Vec::new();
        if rx > 0.0 && ry > 0.0 {
          let corner = |from: P, to: P| Seg::Arc { from, to, rx, ry, phi: 0.0, large: false, sweep: true };
          let pts = [
            [x + rx, y],
            [x + w - rx, y],
            [x + w, y + ry],
            [x + w, y + h - ry],
            [x + w - rx, y + h],
            [x + rx, y + h],
            [x, y + h - ry],
            [x, y + ry],
          ];
          for i in 0..4 {
            let (p, q, r) = (pts[2 * i], pts[2 * i + 1], pts[(2 * i + 2) % 8]);
            segs.push(Seg::Line(p, q));
            segs.push(corner(q, r));
          }
        } else {
          let pts = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
          segs.extend((0..4).map(|i| Seg::Line(pts[i], pts[(i + 1) % 4])));
        }
        emit_subs(&[Sub { segs, closed: true }], &xf, cons, out);
      }
    }
    "circle" => {
      let r = num("r");
      emit_ellipse(&xf, [num("cx"), num("cy")], r, r, cons, out);
    }
    "ellipse" => {
      let (rx, ry) = match (user_len(a, "rx"), user_len(a, "ry")) {
        (Some(rx), Some(ry)) => (rx, ry),
        (Some(r), None) | (None, Some(r)) => (r, r),
        (None, None) => (0.0, 0.0),
      };
      emit_ellipse(&xf, [num("cx"), num("cy")], rx, ry, cons, out);
    }
    "line" => {
      let segs = vec![Seg::Line([num("x1"), num("y1")], [num("x2"), num("y2")])];
      emit_subs(&[Sub { segs, closed: false }], &xf, cons, out);
    }
    "polyline" | "polygon" => {
      let v = numbers(a.get("points").map_or("", String::as_str));
      let pts: Vec<P> = v.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
      let segs = pts.windows(2).map(|w| Seg::Line(w[0], w[1])).collect();
      emit_subs(&[Sub { segs, closed: name == "polygon" }], &xf, cons, out);
    }
    "text" | "image" | "use" | "foreignObject" => {
      skip_count(d, name);
      return skip;
    }
    // defs, clipPath, mask, symbol, marker, pattern, style, metadata, title, editor data, ...
    _ => return skip,
  }
  Frame { xf, construction: cons, skip: false }
}

/// Reads an SVG's geometry in millimetres, y up. Fails for files that aren't SVG.
pub fn read_svg(bytes: &[u8]) -> Result<Drawing> {
  if bytes.starts_with(&[0x1f, 0x8b]) {
    return Err(ExchangeError::fmt("SVG: compressed SVG (.svgz) isn't supported; unzip it first"));
  }
  let text = String::from_utf8_lossy(bytes);
  let text = text.trim_start_matches('\u{feff}');
  let not_svg = || ExchangeError::fmt("not an SVG file (no <svg> root element)");
  let mut reader = quick_xml::Reader::from_str(text);
  reader.config_mut().trim_text(true);
  let mut d = Drawing::default();
  let mut stack: Vec<Frame> = Vec::new();
  let mut seen_root = false;
  loop {
    let ev = match reader.read_event() {
      Ok(ev) => ev,
      Err(_) if !seen_root => return Err(not_svg()),
      Err(e) => return Err(ExchangeError::Xml(format!("SVG: {e}"))),
    };
    let (e, empty) = match ev {
      Event::Start(e) => (e, false),
      Event::Empty(e) => (e, true),
      Event::End(_) => {
        stack.pop();
        if stack.is_empty() && seen_root {
          break;
        }
        continue;
      }
      Event::Eof => break,
      _ => continue,
    };
    let local = e.local_name();
    let name = local.as_ref();
    let a = attrs(&e);
    let parent = match stack.last() {
      Some(f) => *f,
      None => {
        if seen_root {
          break;
        }
        if name != "svg" {
          return Err(ExchangeError::fmt(format!("not an SVG file (the root element is <{name}>)")));
        }
        seen_root = true;
        Frame { xf: root_transform(&a), construction: false, skip: false }
      }
    };
    let frame = if stack.is_empty() {
      // the root: its own transform (SVG 2) on top of the unit mapping
      let own = a.get("transform").and_then(|t| parse_transform(t)).unwrap_or(Xf::ID);
      Frame { xf: parent.xf.mul(own), ..parent }
    } else {
      element(&mut d, parent, name, &a)
    };
    if !empty {
      stack.push(frame);
    } else if stack.is_empty() {
      break;
    }
  }
  if !seen_root {
    return Err(not_svg());
  }
  Ok(d)
}

// ---------------------------------------------------------------------------------------------
// writing

fn num(v: f64) -> String {
  let v = if v.abs() < 5e-10 { 0.0 } else { v };
  format!("{v:.9}").trim_end_matches('0').trim_end_matches('.').to_string()
}

fn pt(p: P) -> String {
  format!("{} {}", num(p[0]), num(p[1]))
}

/// End points of a curve entity (`None` for circles, points and full-turn arcs).
fn ends(e: &Entity) -> Option<(P, P)> {
  match *e {
    Entity::Line { a, b } => Some((a, b)),
    Entity::Bezier { p } => Some((p[0], p[3])),
    Entity::Arc { center, radius, start, end } => {
      let sweep = (end - start).rem_euclid(TAU);
      (sweep > 1e-12 && sweep < TAU - 1e-12)
        .then(|| (Entity::arc_point(center, radius, start), Entity::arc_point(center, radius, end)))
    }
    Entity::Circle { .. } | Entity::Point { .. } => None,
  }
}

/// Points that bound an entity (control points for a Bézier).
fn extent(e: &Entity) -> Vec<P> {
  match *e {
    Entity::Line { a, b } => vec![a, b],
    Entity::Bezier { p } => p.to_vec(),
    Entity::Circle { center, radius } => {
      vec![[center[0] - radius, center[1] - radius], [center[0] + radius, center[1] + radius]]
    }
    Entity::Arc { center, radius, start, end } => {
      let sweep = (end - start).rem_euclid(TAU);
      let mut v = vec![Entity::arc_point(center, radius, start), Entity::arc_point(center, radius, end)];
      for k in 0..4 {
        let a = k as f64 * FRAC_PI_2;
        if (a - start).rem_euclid(TAU) <= sweep || sweep < 1e-12 {
          v.push(Entity::arc_point(center, radius, a));
        }
      }
      v
    }
    Entity::Point { .. } => Vec::new(),
  }
}

/// One `<path>` for a chain of connected entities, each with "reversed".
fn write_chain(s: &mut String, indent: &str, chain: &[(Entity, bool)]) {
  let Some(&(e0, r0)) = chain.first() else { return };
  let Some((a, b)) = ends(&e0) else { return };
  let start = if r0 { b } else { a };
  let end_of = |e: &Entity, rev: bool| ends(e).map(|(a, b)| if rev { a } else { b }).unwrap_or(start);
  let last = chain.last().map_or(start, |(e, r)| end_of(e, *r));
  let closed = chain.len() > 1 && dist(last, start) < JOIN;
  let mut d = format!("M {}", pt(start));
  for (i, (e, rev)) in chain.iter().enumerate() {
    let is_last = i + 1 == chain.len();
    // the closing point is written as the start itself, so `Z` adds nothing
    let to = if closed && is_last { start } else { end_of(e, *rev) };
    match *e {
      Entity::Line { .. } if closed && is_last => {}
      Entity::Line { .. } => {
        let _ = write!(d, " L {}", pt(to));
      }
      Entity::Bezier { p } => {
        let (c1, c2) = if *rev { (p[2], p[1]) } else { (p[1], p[2]) };
        let _ = write!(d, " C {} {} {}", pt(c1), pt(c2), pt(to));
      }
      Entity::Arc { radius, start: a0, end: a1, .. } => {
        let large = (a1 - a0).rem_euclid(TAU) > std::f64::consts::PI;
        // y up inside the flipped group: sweep 1 runs counter-clockwise
        let sweep = !*rev;
        let r = num(radius);
        let _ = write!(d, " A {r} {r} 0 {} {} {}", u8::from(large), u8::from(sweep), pt(to));
      }
      Entity::Circle { .. } | Entity::Point { .. } => {}
    }
  }
  if closed {
    d.push_str(" Z");
  }
  let _ = writeln!(s, "{indent}<path d=\"{d}\"/>");
}

/// End points closer than this (mm) chain into one path.
const JOIN: f64 = 1e-7;

/// Writes entities as `<path>`s and `<circle>`s, joining consecutive connected curves.
fn write_group(s: &mut String, indent: &str, entities: &[Entity]) {
  let mut chain: Vec<(Entity, bool)> = Vec::new();
  for e in entities {
    if let Entity::Point { .. } = e {
      continue;
    }
    let circle = match *e {
      Entity::Circle { center, radius } => Some((center, radius)),
      Entity::Arc { center, radius, .. } if ends(e).is_none() => Some((center, radius)),
      _ => None,
    };
    if let Some((c, r)) = circle {
      write_chain(s, indent, &chain);
      chain.clear();
      let _ = writeln!(s, "{indent}<circle cx=\"{}\" cy=\"{}\" r=\"{}\"/>", num(c[0]), num(c[1]), num(r));
      continue;
    }
    let Some((a, b)) = ends(e) else { continue };
    if let Some(&(last, rev)) = chain.last() {
      let end = ends(&last).map(|(p, q)| if rev { p } else { q });
      if let Some(end) = end {
        if dist(end, a) < JOIN {
          chain.push((*e, false));
          continue;
        }
        if dist(end, b) < JOIN {
          chain.push((*e, true));
          continue;
        }
      }
      // a lone first entity may run backwards to meet the next one
      if chain.len() == 1
        && let Some((s0, _)) = ends(&last)
        && (dist(s0, a) < JOIN || dist(s0, b) < JOIN)
      {
        chain[0].1 = true;
        chain.push((*e, dist(s0, a) >= JOIN));
        continue;
      }
    }
    write_chain(s, indent, &chain);
    chain.clear();
    chain.push((*e, false));
  }
  write_chain(s, indent, &chain);
}

/// Writes entities (millimetres) as a standalone SVG in millimetres, stroke only. Construction
/// entities go in a dashed group with id "construction". Points have no SVG form and are left
/// out.
pub fn write_svg(entities: &[(Entity, bool)]) -> String {
  let pts: Vec<P> =
    entities.iter().flat_map(|(e, _)| extent(e)).filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
  let (mut lo, mut hi) = ([0.0f64; 2], [0.0f64; 2]);
  if let Some(first) = pts.first() {
    (lo, hi) = (*first, *first);
    for p in &pts {
      for k in 0..2 {
        lo[k] = lo[k].min(p[k]);
        hi[k] = hi[k].max(p[k]);
      }
    }
  }
  let margin = 1.0;
  let (x0, y0) = (lo[0] - margin, lo[1] - margin);
  let (w, h) = (hi[0] - lo[0] + 2.0 * margin, hi[1] - lo[1] + 2.0 * margin);
  // mirror y about the middle of the viewBox (what reading undoes)
  let flip = lo[1] + hi[1];
  let mut s = String::new();
  s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- FissionOxide SVG export (mm) -->\n");
  let _ = writeln!(
    s,
    "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\">",
    num(w),
    num(h),
    num(x0),
    num(y0),
    num(w),
    num(h)
  );
  let _ = writeln!(
    s,
    "  <g transform=\"matrix(1 0 0 -1 0 {})\" fill=\"none\" stroke=\"#000000\" stroke-width=\"0.1\" \
     stroke-linecap=\"round\" stroke-linejoin=\"round\">",
    num(flip)
  );
  let normal: Vec<Entity> = entities.iter().filter(|(_, c)| !c).map(|(e, _)| *e).collect();
  let cons: Vec<Entity> = entities.iter().filter(|(_, c)| *c).map(|(e, _)| *e).collect();
  write_group(&mut s, "    ", &normal);
  if !cons.is_empty() {
    s.push_str("    <g id=\"construction\" stroke=\"#808080\" stroke-dasharray=\"1 0.5\">\n");
    write_group(&mut s, "      ", &cons);
    s.push_str("    </g>\n");
  }
  s.push_str("  </g>\n</svg>\n");
  s
}
