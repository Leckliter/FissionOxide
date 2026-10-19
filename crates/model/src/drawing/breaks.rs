//! Broken views: long parts drawn with a stretch cut out and the ends closed up to a small gap,
//! as in Fusion's Break View. The cut is applied where view coordinates become sheet points, so
//! geometry, centre marks and every annotation follow it, while dimensions still read the true
//! (model) length.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakDirection {
  /// Shortens the view along the sheet's horizontal (the break lines stand vertical).
  #[default]
  Horizontal,
  /// Shortens the view along the sheet's vertical.
  Vertical,
}

impl BreakDirection {
  pub fn parse(s: &str) -> Option<BreakDirection> {
    Some(match s.to_ascii_lowercase().as_str() {
      "horizontal" | "h" | "x" => BreakDirection::Horizontal,
      "vertical" | "v" | "y" => BreakDirection::Vertical,
      _ => return None,
    })
  }

  fn axis(self) -> usize {
    match self {
      BreakDirection::Horizontal => 0,
      BreakDirection::Vertical => 1,
    }
  }
}

/// A stretch of a view left out of the drawing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewBreak {
  #[serde(default)]
  pub direction: BreakDirection,
  /// Ends of the removed stretch: model mm along the view's horizontal (or vertical) axis, e.g.
  /// x for a front or top view, y for a right view.
  pub from: f64,
  pub to: f64,
  /// Sheet mm left between the break lines.
  #[serde(default = "gap")]
  pub gap: f64,
}

pub const DEFAULT_GAP: f64 = 6.0;

fn gap() -> f64 {
  DEFAULT_GAP
}

/// A removed stretch along one view axis, in view coordinates, with its sheet gap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Cut {
  pub from: f64,
  pub to: f64,
  pub gap: f64,
}

/// A view's breaks per axis (0 = horizontal), clipped to the view, sorted and merged.
pub(super) fn cuts(breaks: &[ViewBreak], lo: [f64; 2], hi: [f64; 2]) -> [Vec<Cut>; 2] {
  let mut out: [Vec<Cut>; 2] = [Vec::new(), Vec::new()];
  for b in breaks {
    let i = b.direction.axis();
    let (from, to) = (b.from.min(b.to).max(lo[i]), b.from.max(b.to).min(hi[i]));
    if to - from > 1e-9 && b.gap.is_finite() {
      out[i].push(Cut { from, to, gap: b.gap.max(0.0) });
    }
  }
  for list in &mut out {
    list.sort_by(|a, b| a.from.total_cmp(&b.from));
    let mut merged: Vec<Cut> = Vec::new();
    for c in list.drain(..) {
      match merged.last_mut() {
        Some(m) if c.from <= m.to => {
          m.to = m.to.max(c.to);
          m.gap = m.gap.max(c.gap);
        }
        _ => merged.push(c),
      }
    }
    *list = merged;
  }
  out
}

/// View units cut out along one axis.
pub(super) fn removed(cuts: &[Cut]) -> f64 {
  cuts.iter().map(|c| c.to - c.from).sum()
}

/// Sheet mm from the view's low edge `lo` to view coordinate `x` (`k` sheet mm per view unit),
/// with the removed stretches closed up to their gaps. Points inside a removed stretch land in
/// the middle of its gap.
pub(super) fn fold(cuts: &[Cut], lo: f64, x: f64, k: f64) -> f64 {
  let mut shift = 0.0;
  for c in cuts {
    if x >= c.to {
      shift += (c.to - c.from) * k - c.gap;
    } else if x > c.from {
      return (c.from - lo) * k - shift + c.gap / 2.0;
    } else {
      break;
    }
  }
  (x - lo) * k - shift
}

/// Is view coordinate `x` inside a removed stretch?
pub(super) fn inside(cuts: &[Cut], x: f64) -> bool {
  cuts.iter().any(|c| x > c.from + 1e-9 && x < c.to - 1e-9)
}

/// The pieces of a polyline (view coordinates) outside the removed stretches along axis `i`.
pub(super) fn clip(pts: &[[f64; 2]], i: usize, cuts: &[Cut]) -> Vec<Vec<[f64; 2]>> {
  if cuts.is_empty() {
    return if pts.len() > 1 { vec![pts.to_vec()] } else { Vec::new() };
  }
  let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
  let mut cur: Vec<[f64; 2]> = Vec::new();
  for w in pts.windows(2) {
    let (a, b) = (w[0], w[1]);
    let p = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    // split the segment where it crosses a cut's ends
    let mut ts = vec![0.0, 1.0];
    let d = b[i] - a[i];
    if d.abs() > 1e-15 {
      for c in cuts {
        for x in [c.from, c.to] {
          let t = (x - a[i]) / d;
          if t > 0.0 && t < 1.0 {
            ts.push(t);
          }
        }
      }
    }
    ts.sort_by(f64::total_cmp);
    for k in 0..ts.len() - 1 {
      let (t0, t1) = (ts[k], ts[k + 1]);
      if t1 - t0 < 1e-12 {
        continue;
      }
      if inside(cuts, p((t0 + t1) / 2.0)[i]) {
        if cur.len() > 1 {
          out.push(std::mem::take(&mut cur));
        }
        cur.clear();
        continue;
      }
      let (p0, p1) = (p(t0), p(t1));
      if cur.last().is_none_or(|q| (q[0] - p0[0]).hypot(q[1] - p0[1]) > 1e-9) {
        if cur.len() > 1 {
          out.push(std::mem::take(&mut cur));
        }
        cur = vec![p0];
      }
      cur.push(p1);
    }
  }
  if cur.len() > 1 {
    out.push(cur);
  }
  out
}

/// A break line across a view (sheet mm): straight from `lo` to `hi` along the other axis at
/// `pos` on axis `i`, with a zigzag in the middle (ISO 128 / ASME Y14.2 long break line).
pub(super) fn break_line(i: usize, pos: f64, lo: f64, hi: f64) -> Vec<[f64; 2]> {
  let m = (lo + hi) / 2.0;
  let z = 1.5;
  let along = [lo, m - 2.0 * z, m - z, m + z, m + 2.0 * z, hi];
  let across = [0.0, 0.0, z, -z, 0.0, 0.0];
  along.iter().zip(across).map(|(a, c)| if i == 0 { [pos + c, *a] } else { [*a, pos + c] }).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn brk(from: f64, to: f64) -> ViewBreak {
    ViewBreak { direction: BreakDirection::Horizontal, from, to, gap: 6.0 }
  }

  #[test]
  fn fold_closes_up_the_cut() {
    let c = cuts(&[brk(20.0, 80.0)], [0.0, 0.0], [100.0, 10.0]);
    assert_eq!(c[0].len(), 1);
    assert!(c[1].is_empty());
    // at scale 1: 0..20 stays, 80..100 moves left by 60 - 6
    assert_eq!(fold(&c[0], 0.0, 10.0, 1.0), 10.0);
    assert_eq!(fold(&c[0], 0.0, 20.0, 1.0), 20.0);
    assert_eq!(fold(&c[0], 0.0, 80.0, 1.0), 26.0);
    assert_eq!(fold(&c[0], 0.0, 100.0, 1.0), 46.0);
    assert_eq!(fold(&c[0], 0.0, 50.0, 1.0), 23.0);
    assert_eq!(removed(&c[0]), 60.0);
  }

  #[test]
  fn breaks_merge_and_clip() {
    let c = cuts(&[brk(50.0, 70.0), brk(-10.0, 30.0), brk(60.0, 90.0)], [0.0, 0.0], [100.0, 10.0]);
    assert_eq!(c[0], vec![Cut { from: 0.0, to: 30.0, gap: 6.0 }, Cut { from: 50.0, to: 90.0, gap: 6.0 }]);
  }

  #[test]
  fn clip_cuts_lines() {
    let c = cuts(&[brk(20.0, 80.0)], [0.0, 0.0], [100.0, 10.0]);
    let pieces = clip(&[[0.0, 0.0], [100.0, 0.0], [100.0, 10.0]], 0, &c[0]);
    assert_eq!(pieces, vec![vec![[0.0, 0.0], [20.0, 0.0]], vec![[80.0, 0.0], [100.0, 0.0], [100.0, 10.0]]]);
    // a line wholly inside the cut disappears; one along its edge stays
    assert!(clip(&[[30.0, 0.0], [30.0, 10.0]], 0, &c[0]).is_empty());
    assert_eq!(clip(&[[20.0, 0.0], [20.0, 10.0]], 0, &c[0]).len(), 1);
  }
}
