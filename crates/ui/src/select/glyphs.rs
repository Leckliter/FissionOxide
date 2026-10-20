//! Joint glyphs as Fusion draws them over the model: a stem along the joint's Z axis with a flag
//! at its tip (the axis direction) and a ring at the joint origin, with the type's own marks — a
//! turning arrow on the ring for rotation, arrows along the axis for a slide, a square for a
//! planar joint, a second ring for a ball, a cross for a rigid joint.

use egui::{Color32, Pos2, Stroke};
use fox_engine::kernel::glam::{DMat4, DVec3};
use fox_engine::model::JointKind;

/// A glyph in screen space: line segments and filled shapes.
#[derive(Default, Debug)]
pub(crate) struct Glyph {
  pub lines: Vec<[Pos2; 2]>,
  pub fills: Vec<Vec<Pos2>>,
}

impl Glyph {
  /// Distance from `p` to the nearest line (or fill corner).
  pub fn distance(&self, p: Pos2) -> f32 {
    let seg = |[a, b]: &[Pos2; 2]| {
      let ab = *b - *a;
      let t = if ab.length_sq() > 1e-6 { ((p - *a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
      (*a + ab * t - p).length()
    };
    let lines = self.lines.iter().map(seg);
    let fills = self.fills.iter().flat_map(|f| f.iter().map(|q| (*q - p).length()));
    lines.chain(fills).fold(f32::INFINITY, f32::min)
  }

  pub fn paint(&self, painter: &egui::Painter, stroke: Stroke) {
    for l in &self.lines {
      painter.line_segment(*l, stroke);
    }
    for f in &self.fills {
      painter.add(egui::Shape::convex_polygon(f.clone(), stroke.color, Stroke::NONE));
    }
  }
}

/// The glyph of a joint of `kind` whose frame is `frame`; `unit` is the world length of about
/// 22 points on screen at the origin.
pub(crate) fn joint_glyph(kind: JointKind, frame: &DMat4, unit: f64, project: &dyn Fn(DVec3) -> Option<Pos2>) -> Glyph {
  let o = frame.w_axis.truncate();
  let (x, y, z) = (frame.x_axis.truncate(), frame.y_axis.truncate(), frame.z_axis.truncate());
  let at = |lx: f64, ly: f64, lz: f64| project(o + (x * lx + y * ly + z * lz) * unit);
  let mut g = Glyph::default();
  let line = |g: &mut Glyph, a: Option<Pos2>, b: Option<Pos2>| {
    if let (Some(a), Some(b)) = (a, b) {
      g.lines.push([a, b]);
    }
  };
  // a ring in the joint's XY plane, at height `h`, radius `r`
  let ring = |g: &mut Glyph, r: f64, h: f64| {
    const N: usize = 36;
    let pts: Vec<Option<Pos2>> = (0..=N)
      .map(|i| {
        let a = i as f64 / N as f64 * std::f64::consts::TAU;
        at(r * a.cos(), r * a.sin(), h)
      })
      .collect();
    for w in pts.windows(2) {
      if let (Some(a), Some(b)) = (w[0], w[1]) {
        g.lines.push([a, b]);
      }
    }
  };
  // an arrow head at `tip` pointing along local direction `d`, opening in local direction `side`
  let head = |g: &mut Glyph, tip: [f64; 3], d: [f64; 3], side: [f64; 3]| {
    let s = 0.22;
    let back =
      |k: f64| at(tip[0] - d[0] * s + side[0] * k, tip[1] - d[1] * s + side[1] * k, tip[2] - d[2] * s + side[2] * k);
    if let (Some(t), Some(a), Some(b)) = (at(tip[0], tip[1], tip[2]), back(s * 0.6), back(-s * 0.6)) {
      g.fills.push(vec![t, a, b]);
    }
  };
  let r = 0.55;
  let stem = 1.6;
  ring(&mut g, r, 0.0);
  // the stem along Z, with its flag at the tip
  line(&mut g, at(0.0, 0.0, 0.0), at(0.0, 0.0, stem));
  if let (Some(t), Some(a), Some(b)) = (at(0.0, 0.0, stem), at(0.5, 0.0, stem - 0.15), at(0.0, 0.0, stem - 0.35)) {
    g.fills.push(vec![t, a, b]);
  }
  let (rot, slide, slide2, pitch) = kind.freedoms();
  if rot {
    // the turning arrow, on the ring at +X heading round towards +Y
    head(&mut g, [r, 0.12, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
  }
  match kind {
    JointKind::Rigid => {
      let k = r * 0.6;
      line(&mut g, at(-k, -k, 0.0), at(k, k, 0.0));
      line(&mut g, at(-k, k, 0.0), at(k, -k, 0.0));
    }
    JointKind::Slider | JointKind::Cylindrical => {
      // a double arrow along the axis, below the origin too
      line(&mut g, at(0.0, 0.0, 0.0), at(0.0, 0.0, -1.0));
      head(&mut g, [0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
      head(&mut g, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    }
    JointKind::Screw => {
      // a second turn above the first, and the advance along the axis
      ring(&mut g, r * 0.8, 0.45);
      head(&mut g, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    }
    JointKind::PinSlot => {
      line(&mut g, at(-1.1, 0.0, 0.0), at(1.1, 0.0, 0.0));
      head(&mut g, [1.1, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
      head(&mut g, [-1.1, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    }
    JointKind::Planar => {
      let k = 1.0;
      let sq = [at(-k, -k, 0.0), at(k, -k, 0.0), at(k, k, 0.0), at(-k, k, 0.0)];
      for i in 0..4 {
        line(&mut g, sq[i], sq[(i + 1) % 4]);
      }
    }
    JointKind::Ball => {
      // a second ring across the first: a sphere
      const N: usize = 36;
      let pts: Vec<Option<Pos2>> = (0..=N)
        .map(|i| {
          let a = i as f64 / N as f64 * std::f64::consts::TAU;
          at(r * a.cos(), 0.0, r * a.sin())
        })
        .collect();
      for w in pts.windows(2) {
        line(&mut g, w[0], w[1]);
      }
    }
    JointKind::Revolute => {}
  }
  let _ = (slide, slide2, pitch);
  g
}

/// The colours a joint glyph draws in: its ink, and whether it is lit up (hovered here or in the
/// browser; Fusion turns it blue and bolder) or selected.
pub(crate) fn glyph_stroke(lit: bool, selected: bool) -> (Stroke, Option<Stroke>) {
  if lit {
    (Stroke::new(2.5, Color32::from_rgb(0, 150, 255)), Some(Stroke::new(5.0, Color32::from_white_alpha(200))))
  } else if selected {
    (Stroke::new(2.2, Color32::from_rgb(36, 142, 220)), Some(Stroke::new(4.5, Color32::from_white_alpha(170))))
  } else {
    (Stroke::new(1.6, Color32::from_rgb(40, 90, 170)), Some(Stroke::new(3.5, Color32::from_white_alpha(140))))
  }
}

#[cfg(test)]
mod tests {
  use egui::pos2;

  use super::*;

  /// A camera looking down -Y at the origin: world X right, Z up, 10 points per unit.
  fn front(p: DVec3) -> Option<Pos2> {
    Some(pos2(100.0 + p.x as f32 * 10.0, 100.0 - p.z as f32 * 10.0))
  }

  #[test]
  fn a_revolute_draws_a_stem_with_a_flag_and_a_turning_ring() {
    let g = joint_glyph(JointKind::Revolute, &DMat4::IDENTITY, 2.0, &front);
    // the stem runs up the Z axis from the origin, 1.6 units long
    assert!(g.lines.iter().any(|l| l[0] == pos2(100.0, 100.0) && l[1] == pos2(100.0, 68.0)), "{g:?}");
    // the flag at its tip, and the arrow head on the ring
    assert_eq!(g.fills.len(), 2);
    assert!(g.fills[0].contains(&pos2(100.0, 68.0)));
    // the ring is in the XY plane: edge-on from the front, a line across the origin
    assert!(g.lines.iter().all(|l| (l[0].y - 100.0).abs() < 1e-3 || l[0].x == 100.0 || l[1].x == 100.0));
    // picking: on the stem, near; well away, not
    assert!(g.distance(pos2(101.0, 80.0)) < 2.0);
    assert!(g.distance(pos2(160.0, 20.0)) > 20.0);
  }

  #[test]
  fn each_type_has_its_own_marks() {
    let n = |k| {
      let g = joint_glyph(k, &DMat4::IDENTITY, 1.0, &front);
      (g.lines.len(), g.fills.len())
    };
    let rev = n(JointKind::Revolute);
    // a slider: two arrow heads (and no turning arrow), more stem
    assert_eq!(n(JointKind::Slider).1, 3);
    assert!(n(JointKind::Slider).0 > rev.0 - 1);
    assert_eq!(n(JointKind::Cylindrical).1, 4);
    assert_eq!(n(JointKind::Rigid), (rev.0 + 2, 1));
    assert_eq!(n(JointKind::Planar).0, rev.0 + 4);
    assert!(n(JointKind::Ball).0 > rev.0 + 30);
  }
}
