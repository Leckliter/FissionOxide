//! Connected components: dropping specks, filling small holes, and labelling shapes.
//!
//! Foreground regions are 8-connected and background regions 4-connected, matching the rule the
//! boundary follower uses where pixels touch only at a corner. Labelling works on horizontal
//! runs with union-find, so its cost follows the number of runs rather than of pixels.

/// Label of a pixel outside every component.
pub(crate) const NONE: u32 = u32::MAX;

struct Comp {
  area: u64,
  border: bool,
}

struct Run {
  y: usize,
  x0: usize,
  /// Exclusive.
  x1: usize,
}

fn find(parent: &mut [u32], mut a: u32) -> u32 {
  while parent[a as usize] != a {
    let up = parent[parent[a as usize] as usize];
    parent[a as usize] = up;
    a = up;
  }
  a
}

/// Labels the connected regions of pixels equal to `value`, numbered in scan order.
fn label(mask: &[bool], w: usize, h: usize, value: bool, eight: bool) -> (Vec<u32>, Vec<Comp>) {
  let mut runs: Vec<Run> = Vec::new();
  let mut row_start = Vec::with_capacity(h + 1);
  for y in 0..h {
    row_start.push(runs.len());
    let row = &mask[y * w..(y + 1) * w];
    let mut x = 0;
    while x < w {
      if row[x] != value {
        x += 1;
        continue;
      }
      let x0 = x;
      while x < w && row[x] == value {
        x += 1;
      }
      runs.push(Run { y, x0, x1: x });
    }
  }
  row_start.push(runs.len());
  let mut parent: Vec<u32> = (0..runs.len() as u32).collect();
  // diagonal neighbours count for 8-connectivity
  let reach = usize::from(eight);
  for y in 1..h {
    let (mut i, mut j) = (row_start[y - 1], row_start[y]);
    let (pe, ce) = (row_start[y], row_start[y + 1]);
    while i < pe && j < ce {
      let (a, b) = (&runs[i], &runs[j]);
      if a.x0 < b.x1 + reach && b.x0 < a.x1 + reach {
        let (ra, rb) = (find(&mut parent, i as u32), find(&mut parent, j as u32));
        if ra != rb {
          parent[ra.max(rb) as usize] = ra.min(rb);
        }
      }
      if a.x1 < b.x1 { i += 1 } else { j += 1 }
    }
  }
  let mut comp_of = vec![NONE; runs.len()];
  let mut comps: Vec<Comp> = Vec::new();
  let mut labels = vec![NONE; mask.len()];
  for (r, run) in runs.iter().enumerate() {
    let root = find(&mut parent, r as u32) as usize;
    if comp_of[root] == NONE {
      comp_of[root] = comps.len() as u32;
      comps.push(Comp { area: 0, border: false });
    }
    let id = comp_of[root];
    let c = &mut comps[id as usize];
    c.area += (run.x1 - run.x0) as u64;
    c.border |= run.y == 0 || run.y + 1 == h || run.x0 == 0 || run.x1 == w;
    labels[run.y * w + run.x0..run.y * w + run.x1].fill(id);
  }
  (labels, comps)
}

/// Drops shapes smaller than `specks` and fills holes smaller than `holes` (areas in px²).
/// Returns the shape label of every foreground pixel ([`NONE`] for background).
pub(crate) fn clean(mask: &mut [bool], w: usize, h: usize, specks: f64, holes: f64) -> Vec<u32> {
  let (mut labels, comps) = label(mask, w, h, true, true);
  if specks > 0.0 && comps.iter().any(|c| (c.area as f64) < specks) {
    for (m, l) in mask.iter_mut().zip(labels.iter_mut()) {
      if *l != NONE && (comps[*l as usize].area as f64) < specks {
        *m = false;
        *l = NONE;
      }
    }
  }
  if holes > 0.0 {
    let (bg, bcomps) = label(mask, w, h, false, false);
    if bcomps.iter().any(|c| !c.border && (c.area as f64) < holes) {
      for (m, &l) in mask.iter_mut().zip(&bg) {
        if l != NONE {
          let c = &bcomps[l as usize];
          if !c.border && (c.area as f64) < holes {
            *m = true;
          }
        }
      }
      // a filled hole may join shapes (a ring around a disc), so label again
      labels = label(mask, w, h, true, true).0;
    }
  }
  labels
}
