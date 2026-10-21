//! Planning a cut: kerf compensation, leads, micro-joints and the order of the contours.

use cavalier_contours::polyline::{PlineSource, PlineSourceMut, PlineVertex, Polyline};
use serde::{Deserialize, Serialize};

use crate::{Contour, P, Seg, add, dist, left, nesting, scale, sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeadKind {
  None,
  /// Straight, square to the contour.
  Line,
  /// A quarter circle tangent to the contour.
  #[default]
  Arc,
}

impl LeadKind {
  pub fn parse(s: &str) -> Option<LeadKind> {
    match s.trim().to_ascii_lowercase().as_str() {
      "none" | "off" | "" => Some(LeadKind::None),
      "line" | "linear" | "straight" => Some(LeadKind::Line),
      "arc" => Some(LeadKind::Arc),
      _ => None,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lead {
  #[serde(default)]
  pub kind: LeadKind,
  /// Line length, or the arc's radius (mm). Shortened where a hole is too small for it.
  #[serde(default = "lead_len")]
  pub length: f64,
}

fn lead_len() -> f64 {
  2.0
}

impl Default for Lead {
  fn default() -> Self {
    Lead { kind: LeadKind::Arc, length: lead_len() }
  }
}

/// Process settings for one material and thickness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CutParams {
  /// Width of the cut (mm); the path runs half of it off the part, into the scrap.
  pub kerf: f64,
  /// Offset the path by half the kerf (off: the path follows the part, for machines that
  /// compensate themselves).
  pub compensate: bool,
  /// Cutting feed (mm/min).
  pub feed: f64,
  /// Laser power (percent of the machine's maximum).
  pub power: f64,
  /// Dwell after turning on, before moving (seconds).
  pub pierce_delay: f64,
  /// Torch height while piercing and while cutting (mm, plasma).
  pub pierce_height: f64,
  pub cut_height: f64,
  /// Height for rapid moves (mm, machines with Z).
  pub safe_height: f64,
  pub lead_in: Lead,
  pub lead_out: Lead,
  /// Distance cut past the start before the lead-out (mm).
  pub overcut: f64,
  /// Micro-joints per part outline: short uncut bridges that keep the part in the sheet.
  pub tabs: u32,
  /// Width of each micro-joint (mm).
  pub tab_width: f64,
  /// Put micro-joints in holes too.
  pub tabs_on_holes: bool,
  /// The part is on the right of the direction of travel (outlines clockwise, holes
  /// counter-clockwise), the good side of a torch with a clockwise swirl; off reverses it.
  pub part_on_right: bool,
  /// Holes smaller than this diameter (mm) are cut at `small_hole_feed` × the feed.
  pub small_hole_diameter: f64,
  pub small_hole_feed: f64,
  /// Material number for controls with a material table (QtPlasmaC M190 Pn; 0 = none).
  pub material: u32,
  /// Where lead-ins start, picked per contour: each point goes to the nearest contour (within
  /// [`LEAD_SNAP`]), which then starts at its closest point instead of the one nearest the
  /// previous cut.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub lead_points: Vec<P>,
}

/// How far a lead point may be from its contour (mm); farther ones are ignored with a warning.
pub const LEAD_SNAP: f64 = 10.0;

impl Default for CutParams {
  fn default() -> Self {
    CutParams {
      kerf: 0.2,
      compensate: true,
      feed: 1000.0,
      power: 100.0,
      pierce_delay: 0.0,
      pierce_height: 3.8,
      cut_height: 1.5,
      safe_height: 10.0,
      lead_in: Lead::default(),
      lead_out: Lead { kind: LeadKind::None, length: lead_len() },
      overcut: 0.0,
      tabs: 0,
      tab_width: 0.5,
      tabs_on_holes: false,
      part_on_right: true,
      small_hole_diameter: 0.0,
      small_hole_feed: 0.6,
      material: 0,
      lead_points: Vec::new(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegKind {
  LeadIn,
  Cut,
  LeadOut,
}

/// One run of the beam, torch or jet: on at the first segment's start, off at the end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pass {
  /// Index of the contour in [`Plan::paths`].
  pub contour: usize,
  pub hole: bool,
  /// Feed as a fraction of the process feed (small holes go slower).
  pub feed_scale: f64,
  pub segs: Vec<(Seg, SegKind)>,
}

impl Pass {
  pub fn start(&self) -> P {
    self.segs.first().map(|s| s.0.a).unwrap_or_default()
  }

  pub fn end(&self) -> P {
    self.segs.last().map(|s| s.0.b).unwrap_or_default()
  }
}

/// A cut, ready for a post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
  pub passes: Vec<Pass>,
  /// The compensated contours, in cutting order (outline or hole each).
  pub paths: Vec<(Contour, bool)>,
  /// Things the user should know (holes smaller than the kerf, leads that didn't fit).
  pub warnings: Vec<String>,
  /// The sheet the parts are cut from (lower-left and upper-right corners), when nested; drawn
  /// on its own layer in a DXF.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sheet: Option<(P, P)>,
}

impl Plan {
  /// Length cut with the beam on (mm), leads included.
  pub fn cut_length(&self) -> f64 {
    self.passes.iter().flat_map(|p| p.segs.iter()).map(|s| s.0.length()).sum()
  }

  /// Length of the moves between passes (mm), from and back to the origin.
  pub fn rapid_length(&self) -> f64 {
    let mut at = [0.0, 0.0];
    let mut l = 0.0;
    for p in &self.passes {
      l += dist(at, p.start());
      at = p.end();
    }
    l + dist(at, [0.0, 0.0])
  }

  pub fn pierces(&self) -> usize {
    self.passes.len()
  }

  /// Estimated time (seconds) at a rapid rate (mm/min).
  pub fn time(&self, params: &CutParams, rapid: f64) -> f64 {
    let cut: f64 = self
      .passes
      .iter()
      .map(|p| p.segs.iter().map(|s| s.0.length()).sum::<f64>() / (params.feed * p.feed_scale).max(1e-6))
      .sum();
    (cut + self.rapid_length() / rapid.max(1e-6)) * 60.0 + self.pierces() as f64 * params.pierce_delay
  }
}

fn to_pline(c: &Contour) -> Polyline<f64> {
  let mut p = Polyline::new_closed();
  for s in &c.segs {
    p.add_vertex(PlineVertex::new(s.a[0], s.a[1], s.bulge));
  }
  p
}

fn from_pline(p: &Polyline<f64>) -> Contour {
  let n = p.vertex_count();
  let segs = (0..n)
    .map(|i| {
      let (v, w) = (p.at(i), p.at((i + 1) % n));
      Seg { a: [v.x, v.y], b: [w.x, w.y], bulge: v.bulge }
    })
    .collect();
  Contour { segs }
}

/// Plans the cut of closed contours (any orientation, holes found by nesting).
pub fn plan(contours: &[Contour], params: &CutParams) -> Plan {
  let mut warnings = Vec::new();
  let (depth, _) = nesting(contours);
  // compensated paths: (contour, hole, index of the source contour)
  let mut paths: Vec<(Contour, bool, usize)> = Vec::new();
  for (i, c) in contours.iter().enumerate() {
    let hole = depth[i] % 2 == 1;
    // part on the right: outlines clockwise, holes counter-clockwise
    let want_ccw = hole == params.part_on_right;
    let c = if (c.area() > 0.0) == want_ccw { c.clone() } else { c.reversed() };
    // the scrap is on the left when the part is on the right; cavalier offsets to the left for
    // positive distances
    let half = if params.compensate { params.kerf.max(0.0) / 2.0 } else { 0.0 };
    if half <= 1e-9 {
      paths.push((c, hole, i));
      continue;
    }
    let off = if params.part_on_right { half } else { -half };
    let pl = to_pline(&c);
    let res: Vec<Polyline<f64>> = pl.parallel_offset(off);
    if res.is_empty() {
      let (lo, hi) = c.bounds();
      warnings.push(format!(
        "a {} of {:.2} x {:.2} mm is narrower than the kerf and was left out",
        if hole { "hole" } else { "part" },
        hi[0] - lo[0],
        hi[1] - lo[1]
      ));
    }
    for r in res {
      if r.vertex_count() >= 2 {
        paths.push((from_pline(&r), hole, i));
      }
    }
  }

  // lead positions picked by the user: the closest point to each contour
  let mut wanted: Vec<Option<(P, f64)>> = vec![None; contours.len()];
  for &q in &params.lead_points {
    let best = contours.iter().enumerate().map(|(i, c)| (i, c.closest(q).2)).min_by(|a, b| a.1.total_cmp(&b.1));
    match best {
      Some((i, d)) if d <= LEAD_SNAP => {
        if wanted[i].is_none_or(|w| d < w.1) {
          wanted[i] = Some((q, d));
        }
      }
      _ => warnings.push(format!(
        "the lead point ({:.1}, {:.1}) is more than {LEAD_SNAP} mm from every contour; ignored",
        q[0], q[1]
      )),
    }
  }

  // contours that must be cut before each one: everything inside it
  let src: Vec<Contour> = paths.iter().map(|p| p.0.clone()).collect();
  let (_, parent) = nesting(&src);
  let mut waiting = vec![0usize; paths.len()];
  for i in 0..paths.len() {
    let mut p = parent[i];
    while let Some(q) = p {
      waiting[q] += 1;
      p = parent[q];
    }
  }

  let mut done = vec![false; paths.len()];
  let mut at: P = [0.0, 0.0];
  let mut passes = Vec::new();
  let mut ordered = Vec::new();
  for _ in 0..paths.len() {
    let mut best: Option<(usize, usize, f64, f64)> = None;
    for (i, (c, _, src)) in paths.iter().enumerate() {
      if done[i] || waiting[i] > 0 {
        continue;
      }
      let (k, f, d) = match wanted[*src] {
        Some((q, _)) => {
          let (k, f, _) = c.closest(q);
          (k, f, dist(at, c.segs[k].at(f)))
        }
        None => start_point(c, at),
      };
      if best.is_none_or(|b| d < b.3) {
        best = Some((i, k, f, d));
      }
    }
    let Some((i, k, f, _)) = best else { break };
    done[i] = true;
    let mut p = parent[i];
    while let Some(q) = p {
      waiting[q] -= 1;
      p = parent[q];
    }
    let (c, hole, _) = &paths[i];
    let c = c.start_at(k, f);
    let idx = ordered.len();
    let mut ps = contour_passes(&c, *hole, idx, params, &mut warnings);
    if let Some(l) = ps.last() {
      at = l.end();
    }
    passes.append(&mut ps);
    ordered.push((c, *hole));
  }
  Plan { passes, paths: ordered, warnings, sheet: None }
}

/// Where to start a contour coming from `at`: the closest point, unless that is a sharp corner
/// (the lead's direction would be ambiguous), then the middle of the closest segment.
fn start_point(c: &Contour, at: P) -> (usize, f64, f64) {
  let (k, f, d) = c.closest(at);
  let n = c.segs.len();
  let corner = |v: usize| {
    let (a, b) = (c.segs[(v + n - 1) % n], c.segs[v]);
    crate::dot(a.tangent(1.0), b.tangent(0.0)) < 0.985
  };
  let len = c.segs[k].length();
  let at_corner = (f * len < 1e-6 && corner(k)) || ((1.0 - f) * len < 1e-6 && corner((k + 1) % n));
  if !at_corner {
    return (k, f, d);
  }
  let mut best = (k, 0.5, f64::INFINITY);
  for (i, s) in c.segs.iter().enumerate() {
    let d = dist(s.at(0.5), at);
    if d < best.2 {
      best = (i, 0.5, d);
    }
  }
  best
}

/// Segments from distance `from` to `to` along a chain.
fn slice(segs: &[Seg], from: f64, to: f64) -> Vec<Seg> {
  let mut out = Vec::new();
  let mut s0 = 0.0;
  for s in segs {
    let l = s.length();
    let s1 = s0 + l;
    if s1 > from + 1e-9 && s0 < to - 1e-9 && l > 1e-12 {
      let a = ((from - s0) / l).clamp(0.0, 1.0);
      let b = ((to - s0) / l).clamp(0.0, 1.0);
      let mut piece = *s;
      if b < 1.0 {
        piece = piece.split(b).0;
      }
      if a > 0.0 {
        piece = piece.split(a / b).1;
      }
      out.push(piece);
    }
    s0 = s1;
  }
  out
}

/// The contour's passes: lead-in, the cut (broken at micro-joints), overcut and lead-out.
fn contour_passes(c: &Contour, hole: bool, idx: usize, p: &CutParams, warnings: &mut Vec<String>) -> Vec<Pass> {
  let total = c.length();
  let feed_scale = if hole
    && p.small_hole_diameter > 0.0
    && 2.0 * (c.area().abs() / std::f64::consts::PI).sqrt() < p.small_hole_diameter
  {
    p.small_hole_feed.clamp(0.05, 1.0)
  } else {
    1.0
  };
  // scrap side: left of travel when the part is on the right
  let side = if p.part_on_right { 1.0 } else { -1.0 };
  let is_scrap = |q: P| c.contains(q) == hole;

  // runs of cutting between micro-joints
  let mut runs: Vec<(f64, f64)> = vec![(0.0, total)];
  let tabs = if hole && !p.tabs_on_holes { 0 } else { p.tabs };
  if tabs > 0 && p.tab_width > 0.0 {
    if p.tab_width * tabs as f64 > total * 0.5 {
      warnings.push(format!("micro-joints don't fit on a contour {total:.1} mm long; left out"));
    } else {
      runs.clear();
      let mut from = 0.0;
      for t in 0..tabs {
        let mid = total * (t as f64 + 0.5) / tabs as f64;
        runs.push((from, mid - p.tab_width / 2.0));
        from = mid + p.tab_width / 2.0;
      }
      runs.push((from, total));
    }
  }

  let mut passes = Vec::new();
  let last = runs.len() - 1;
  for (r, &(from, to)) in runs.iter().enumerate() {
    let mut segs: Vec<(Seg, SegKind)> = slice(&c.segs, from, to).into_iter().map(|s| (s, SegKind::Cut)).collect();
    if segs.is_empty() {
      continue;
    }
    if r == 0 {
      let s = segs[0].0;
      if let Some(l) = lead(s.a, s.tangent(0.0), side, p.lead_in, true, &is_scrap) {
        segs.insert(0, (l, SegKind::LeadIn));
      } else if p.lead_in.kind != LeadKind::None {
        warnings.push(format!("no room for a lead-in on contour {}; it starts on the contour", idx + 1));
      }
    }
    if r == last {
      if p.overcut > 0.0 {
        let over = slice(&c.segs, 0.0, p.overcut.min(total));
        segs.extend(over.into_iter().map(|s| (s, SegKind::Cut)));
      }
      let s = segs[segs.len() - 1].0;
      if let Some(l) = lead(s.b, s.tangent(1.0), side, p.lead_out, false, &is_scrap) {
        segs.push((l, SegKind::LeadOut));
      }
    }
    passes.push(Pass { contour: idx, hole, feed_scale, segs });
  }
  passes
}

/// A lead into (`into`) or out of point `s` travelling along `t`, on the scrap side, shortened
/// until it stays in the scrap.
fn lead(s: P, t: P, side: f64, l: Lead, into: bool, is_scrap: &dyn Fn(P) -> bool) -> Option<Seg> {
  if l.kind == LeadKind::None || l.length <= 0.0 {
    return None;
  }
  let n = scale(left(t), side);
  let mut len = l.length;
  for _ in 0..8 {
    let seg = match (l.kind, into) {
      (LeadKind::Line, true) => Seg::line(add(s, scale(n, len)), s),
      (LeadKind::Line, false) => Seg::line(s, add(s, scale(n, len))),
      // quarter circle around a centre on the scrap side, tangent to the travel at s
      (_, true) => {
        let c = add(s, scale(n, len));
        Seg { a: sub(c, scale(t, len)), b: s, bulge: side * (std::f64::consts::PI / 8.0).tan() }
      }
      (_, false) => {
        let c = add(s, scale(n, len));
        Seg { a: s, b: add(c, scale(t, len)), bulge: side * (std::f64::consts::PI / 8.0).tan() }
      }
    };
    // sample away from the contour end of the lead
    let ok = [0.0, 0.25, 0.5, 0.75].iter().all(|&f| is_scrap(seg.at(if into { f } else { 1.0 - f })));
    if ok {
      return Some(seg);
    }
    len /= 2.0;
    if len < 0.05 {
      break;
    }
  }
  None
}
