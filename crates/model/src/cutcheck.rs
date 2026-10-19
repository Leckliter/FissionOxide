//! Checks a flat part against the rules shops cut by: what a laser, plasma or waterjet can cut
//! cleanly, and what survives being bent. Works on closed 2D contours (a flat pattern's outline
//! and holes), so it serves any cut file.
//!
//! The defaults are common sheet metal guidelines, not any one shop's limits:
//! - holes and cut-outs at least as wide as the material is thick;
//! - material between a hole and an edge, or between two holes, at least the thickness;
//! - holes at least 2.5 × thickness + bend radius from a bend line, or the bend pulls them out of
//!   shape;
//! - the part fits the sheet, when a sheet size is given.

use serde::Serialize;

pub type P2 = [f64; 2];

/// One closed contour: the part's outline or a hole (or cut-out) in it.
#[derive(Debug, Clone)]
pub struct Contour {
  pub points: Vec<P2>,
  pub hole: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
  /// Smallest hole or cut-out width (mm).
  pub min_hole: f64,
  /// Least material between a hole and an edge or another hole (mm).
  pub min_web: f64,
  /// Least distance from a hole to a bend line (mm).
  pub bend_clearance: f64,
  /// Largest part the sheet takes (mm, either way round), if any.
  pub sheet: Option<P2>,
}

impl Rules {
  /// The common guidelines for material `thickness` bent at inside radius `bend_radius`.
  pub fn for_sheet(thickness: f64, bend_radius: f64) -> Rules {
    Rules { min_hole: thickness, min_web: thickness, bend_clearance: 2.5 * thickness + bend_radius, sheet: None }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
  /// small_hole, thin_web, near_bend, too_big
  pub kind: &'static str,
  pub message: String,
  /// Where, in the flat's coordinates.
  pub at: P2,
  /// What was measured, and the rule's limit (mm).
  pub value: f64,
  pub limit: f64,
}

/// Everything about the part that breaks the rules; empty when it is fine to cut.
pub fn check(contours: &[Contour], bends: &[(P2, P2)], rules: &Rules) -> Vec<Finding> {
  let mut out = Vec::new();
  let tol = 1e-6;
  for c in contours.iter().filter(|c| c.hole) {
    let (w, at) = width(&c.points);
    if w + tol < rules.min_hole {
      out.push(Finding {
        kind: "small_hole",
        message: format!("a hole {w:.2} mm across is smaller than {:.2} mm", rules.min_hole),
        at,
        value: w,
        limit: rules.min_hole,
      });
    }
    for (a, b) in bends {
      let (d, at) = distance_to_segment(&c.points, *a, *b);
      if d + tol < rules.bend_clearance {
        out.push(Finding {
          kind: "near_bend",
          message: format!("a hole {d:.2} mm from a bend line needs {:.2} mm", rules.bend_clearance),
          at,
          value: d,
          limit: rules.bend_clearance,
        });
      }
    }
  }
  for (i, a) in contours.iter().enumerate() {
    for b in contours.iter().skip(i + 1) {
      if !(a.hole || b.hole) {
        continue;
      }
      let (d, at) = distance_between(&a.points, &b.points);
      if d + tol < rules.min_web {
        let what = if a.hole && b.hole { "two holes" } else { "a hole and the edge" };
        out.push(Finding {
          kind: "thin_web",
          message: format!("{d:.2} mm of material between {what} is less than {:.2} mm", rules.min_web),
          at,
          value: d,
          limit: rules.min_web,
        });
      }
    }
  }
  if let Some([sw, sh]) = rules.sheet {
    let (lo, hi) = bounds(contours.iter().filter(|c| !c.hole).flat_map(|c| c.points.iter()));
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    let fits = |a: f64, b: f64| a <= sw + tol && b <= sh + tol;
    if !(fits(w, h) || fits(h, w)) {
      out.push(Finding {
        kind: "too_big",
        message: format!("the part is {w:.1} x {h:.1} mm; the sheet is {sw:.1} x {sh:.1} mm"),
        at: lo,
        value: w.max(h),
        limit: sw.max(sh),
      });
    }
  }
  out
}

/// Signed area of a closed polyline (positive counter-clockwise).
pub fn area(pts: &[P2]) -> f64 {
  segments(pts).map(|(a, b)| a[0] * b[1] - b[0] * a[1]).sum::<f64>() / 2.0
}

fn bounds<'a>(pts: impl Iterator<Item = &'a P2>) -> (P2, P2) {
  pts.fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| {
    ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
  })
}

/// The contour's narrowest width (the least distance between parallel lines enclosing it, over
/// 180 directions) and its centre.
fn width(pts: &[P2]) -> (f64, P2) {
  let (lo, hi) = bounds(pts.iter());
  let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
  let w = (0..180)
    .map(|k| {
      let a = (k as f64).to_radians();
      let (c, s) = (a.cos(), a.sin());
      let proj = pts.iter().map(|p| p[0] * c + p[1] * s);
      let (mn, mx) = proj.fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), v| (mn.min(v), mx.max(v)));
      mx - mn
    })
    .fold(f64::INFINITY, f64::min);
  (w, centre)
}

fn closest_on(p: P2, a: P2, b: P2) -> P2 {
  let ab = [b[0] - a[0], b[1] - a[1]];
  let l2 = ab[0] * ab[0] + ab[1] * ab[1];
  let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
  [a[0] + ab[0] * t, a[1] + ab[1] * t]
}

fn dist(a: P2, b: P2) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

fn segments(pts: &[P2]) -> impl Iterator<Item = (P2, P2)> + '_ {
  (0..pts.len()).map(move |i| (pts[i], pts[(i + 1) % pts.len()]))
}

/// Least distance between two closed contours (they don't cross in a valid part) and the
/// midpoint of where it is.
fn distance_between(a: &[P2], b: &[P2]) -> (f64, P2) {
  let mut best = (f64::INFINITY, [0.0, 0.0]);
  for (p0, p1) in segments(a) {
    for (q0, q1) in segments(b) {
      // for segments that don't cross, the closest pair has an end on one of them
      for (p, s0, s1) in [(p0, q0, q1), (p1, q0, q1), (q0, p0, p1), (q1, p0, p1)] {
        let c = closest_on(p, s0, s1);
        let d = dist(p, c);
        if d < best.0 {
          best = (d, [(p[0] + c[0]) / 2.0, (p[1] + c[1]) / 2.0]);
        }
      }
    }
  }
  best
}

fn distance_to_segment(pts: &[P2], a: P2, b: P2) -> (f64, P2) {
  distance_between(pts, &[a, b])
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rect(x0: f64, y0: f64, x1: f64, y1: f64, hole: bool) -> Contour {
    Contour { points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], hole }
  }

  fn circle(cx: f64, cy: f64, r: f64) -> Contour {
    Contour {
      points: (0..72).map(|k| (k as f64 * 5f64).to_radians()).map(|a| [cx + r * a.cos(), cy + r * a.sin()]).collect(),
      hole: true,
    }
  }

  #[test]
  fn a_good_part_passes() {
    let part = [rect(0.0, 0.0, 100.0, 60.0, false), circle(30.0, 30.0, 5.0), circle(70.0, 30.0, 5.0)];
    let bends = [([0.0, 50.0], [100.0, 50.0])];
    assert!(check(&part, &bends, &Rules::for_sheet(2.0, 2.0)).is_empty());
  }

  #[test]
  fn finds_what_shops_reject() {
    let rules = Rules { sheet: Some([90.0, 90.0]), ..Rules::for_sheet(2.0, 2.0) };
    let part = [
      rect(0.0, 0.0, 100.0, 60.0, false),
      circle(20.0, 20.0, 0.75), // 1.5 mm across in 2 mm material
      circle(50.0, 2.5, 1.5),   // 1 mm from the edge
      circle(80.0, 20.0, 3.0),  // 1 mm from the next hole
      circle(87.0, 20.0, 3.0),  //
      circle(50.0, 45.0, 2.0),  // 3 mm from the bend at y 50 (needs 7)
    ];
    let bends = [([0.0, 50.0], [100.0, 50.0])];
    let f = check(&part, &bends, &rules);
    let kinds: Vec<&str> = f.iter().map(|f| f.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "small_hole").count(), 1, "{f:#?}");
    assert_eq!(kinds.iter().filter(|k| **k == "thin_web").count(), 2, "{f:#?}");
    assert_eq!(kinds.iter().filter(|k| **k == "near_bend").count(), 1, "{f:#?}");
    assert_eq!(kinds.iter().filter(|k| **k == "too_big").count(), 1, "{f:#?}");
    let edge = f.iter().find(|f| f.kind == "thin_web" && f.message.contains("edge")).unwrap();
    assert!((edge.value - 1.0).abs() < 0.01 && (edge.at[0] - 50.0).abs() < 0.5, "{edge:?}");
    let bend = f.iter().find(|f| f.kind == "near_bend").unwrap();
    assert!((bend.value - 3.0).abs() < 0.01 && (bend.limit - 7.0).abs() < 1e-9, "{bend:?}");
  }
}
