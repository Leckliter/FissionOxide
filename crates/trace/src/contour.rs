//! Boundary following on pixel corners, and corner detection.
//!
//! Every edge between a foreground and a background pixel is walked with the foreground on the
//! left, so outer boundaries come out counter-clockwise and holes clockwise (y up), and every
//! contour is closed by construction. Where two foreground pixels touch only diagonally the walk
//! turns right, joining them (foreground 8-connected), and that vertex is replaced by two points
//! [`CHAMFER`] px along the incoming and outgoing edges. The two passes through such a vertex
//! thus stay about 0.35 px apart: no contour ever touches itself or another.

use crate::P;
use crate::clean::NONE;

/// Inset of the two points that replace a diagonal-touch vertex (exact in binary).
pub(crate) const CHAMFER: f64 = 0.25;
/// Outline steps on each side over which a bend's angle is measured.
const CORNER_REACH: usize = 4;
/// A corner must be the sharpest bend within this many steps.
const CORNER_WINDOW: usize = 2;

/// One traced boundary.
pub(crate) struct Raw {
  /// Vertices in order: pixel corners one unit apart, or chamfer pairs.
  pub v: Vec<P>,
  /// 1 = first point of a chamfer pair, 2 = second, 0 = an ordinary pixel corner.
  pub pair: Vec<u8>,
  pub hole: bool,
}

type V = (i64, i64);

fn left(d: V) -> V {
  (-d.1, d.0)
}

fn right(d: V) -> V {
  (d.1, -d.0)
}

/// The pixel ahead of vertex `v` (moving along `d`) on `side`.
fn ahead(v: V, d: V, side: V) -> V {
  ((2 * v.0 + d.0 + side.0 - 1).div_euclid(2), (2 * v.1 + d.1 + side.1 - 1).div_euclid(2))
}

/// All boundaries of `mask` (y-up rows), each outer one followed by its holes.
pub(crate) fn trace(mask: &[bool], labels: &[u32], w: usize, h: usize) -> Vec<Raw> {
  let fg =
    |p: V| p.0 >= 0 && p.1 >= 0 && (p.0 as usize) < w && (p.1 as usize) < h && mask[p.1 as usize * w + p.0 as usize];
  // visited flags: horizontal edges (x,y)-(x+1,y) and vertical edges (x,y)-(x,y+1)
  let mut hvis = vec![false; w * (h + 1)];
  let mut vvis = vec![false; (w + 1) * h];
  let mut found: Vec<(u32, Raw)> = Vec::new();
  for y in 0..=h {
    for x in 0..w {
      if hvis[y * w + x] {
        continue;
      }
      let (xi, yi) = (x as i64, y as i64);
      let (above, below) = (fg((xi, yi)), fg((xi, yi - 1)));
      if above == below {
        continue;
      }
      let (start, d0, pix) = if above { ((xi, yi), (1, 0), (x, y)) } else { ((xi + 1, yi), (-1, 0), (x, y - 1)) };
      let label = labels.get(pix.1 * w + pix.0).copied().unwrap_or(NONE);
      mark(&mut hvis, &mut vvis, w, start, d0);
      let (mut v, mut din) = ((start.0 + d0.0, start.1 + d0.1), d0);
      let (mut pts, mut pair) = (Vec::new(), Vec::new());
      loop {
        let (l, r) = (left(din), right(din));
        let (al, ar) = (fg(ahead(v, din, l)), fg(ahead(v, din, r)));
        let dout = if ar {
          r
        } else if al {
          din
        } else {
          l
        };
        if ar && !al {
          // diagonal touch: cut the corner on this pass's side
          pts.push([v.0 as f64 - din.0 as f64 * CHAMFER, v.1 as f64 - din.1 as f64 * CHAMFER]);
          pts.push([v.0 as f64 + dout.0 as f64 * CHAMFER, v.1 as f64 + dout.1 as f64 * CHAMFER]);
          pair.extend([1, 2]);
        } else {
          pts.push([v.0 as f64, v.1 as f64]);
          pair.push(0);
        }
        if v == start && dout == d0 {
          break;
        }
        mark(&mut hvis, &mut vvis, w, v, dout);
        v = (v.0 + dout.0, v.1 + dout.1);
        din = dout;
      }
      let hole = signed_area(&pts) < 0.0;
      found.push((label, Raw { v: pts, pair, hole }));
    }
  }
  // group by shape, outer boundary first (stable, so holes keep scan order)
  found.sort_by_key(|(label, raw)| (*label, raw.hole));
  found.into_iter().map(|(_, r)| r).collect()
}

/// Flags the edge leaving vertex `v` along `d` as walked.
fn mark(hvis: &mut [bool], vvis: &mut [bool], w: usize, v: V, d: V) {
  let (x, y) = (v.0 as usize, v.1 as usize);
  match d {
    (1, 0) => hvis[y * w + x] = true,
    (-1, 0) => hvis[y * w + x - 1] = true,
    (0, 1) => vvis[y * (w + 1) + x] = true,
    _ => vvis[(y - 1) * (w + 1) + x] = true,
  }
}

pub(crate) fn signed_area(pts: &[P]) -> f64 {
  let n = pts.len();
  (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>() / 2.0
}

/// Which vertices are sharp corners: the bend angle between the outline [`CORNER_REACH`] steps
/// back and ahead is below `max_angle` degrees, and no sharper within [`CORNER_WINDOW`] steps.
/// Both points of a chamfer pair are corners if either is.
pub(crate) fn corners(raw: &Raw, max_angle: f64) -> Vec<bool> {
  let v = &raw.v;
  let n = v.len();
  if n < 3 {
    return vec![true; n];
  }
  let k = CORNER_REACH.min((n - 1) / 2).max(1);
  let theta: Vec<f64> = (0..n)
    .map(|i| {
      let (p, a, b) = (v[i], v[(i + n - k) % n], v[(i + k) % n]);
      let (u, w) = ([a[0] - p[0], a[1] - p[1]], [b[0] - p[0], b[1] - p[1]]);
      (u[0] * w[1] - u[1] * w[0]).abs().atan2(u[0] * w[0] + u[1] * w[1]).to_degrees()
    })
    .collect();
  let win = CORNER_WINDOW.min(k);
  let mut corner: Vec<bool> = (0..n)
    .map(|i| {
      theta[i] < max_angle && (1..=win).all(|d| theta[i] <= theta[(i + d) % n] && theta[i] <= theta[(i + n - d) % n])
    })
    .collect();
  for i in 0..n {
    if corner[i] {
      match raw.pair[i] {
        1 => corner[(i + 1) % n] = true,
        2 => corner[(i + n - 1) % n] = true,
        _ => {}
      }
    }
  }
  // a pair's second point can be set by its first after the loop passed it
  for i in 0..n {
    if raw.pair[i] == 2 && corner[i] {
      corner[(i + n - 1) % n] = true;
    }
  }
  corner
}
