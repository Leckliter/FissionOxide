//! Threads: merging the strips a helical cut leaves on a cylinder. The C++ side is
//! `cpp/threads.cpp`.

use glam::DVec3;

use crate::ffi::ffi as sys;
use crate::{Result, Shape};

impl Shape {
  /// Merges faces that lie on the cylinder of `radius` about the axis through `origin` along
  /// `axis`, within `span` along it, and meet only along edges between them (a helical groove
  /// leaves the cylinder as a strip per turn, split at the surface's seam) into one face. Other
  /// faces, even coaxial ones, are left alone. A shape without such faces, or that the merge
  /// would leave invalid, comes back unchanged.
  pub fn merge_cylinder_strips(&self, origin: DVec3, axis: DVec3, radius: f64, span: (f64, f64)) -> Result<Shape> {
    let a = axis.normalize();
    let ax = [origin.x, origin.y, origin.z, a.x, a.y, a.z];
    Shape::wrap(sys::merge_cylinder_strips(self.get()?, &ax, radius, span.0.min(span.1), span.0.max(span.1))?)
  }
}

#[cfg(test)]
mod tests {
  use fox_sketch::{Loop, Profile, Seg};
  use glam::DVec3;

  use crate::{BoolOp, Plane, Shape, SurfaceKind};

  #[test]
  fn helical_groove_leaves_one_crest_face() {
    let rod = Shape::cylinder(DVec3::ZERO, DVec3::Z, 5.0, 12.0).unwrap();
    // a groove 0.5 wide and 0.75 deep, swept from below the rod to above it and cut square at its ends
    let sec = Plane { origin: DVec3::new(4.75, 0.0, -2.0), x: DVec3::X, y: DVec3::Z };
    let pts = [[-0.5, -0.25], [0.5, -0.25], [0.5, 0.25], [-0.5, 0.25]];
    let outer = Loop::new((0..4).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % 4] }).collect());
    let face = Shape::face(&sec, &Profile { outer, holes: vec![] }).unwrap();
    let groove = face.helix_sweep(DVec3::new(0.0, 0.0, -2.0), DVec3::Z, DVec3::X, 4.75, 2.0, 16.0, false).unwrap();
    let slab = Shape::cylinder(DVec3::ZERO, DVec3::Z, 6.0, 12.0).unwrap();
    let cut = rod.boolean(&groove.boolean(&slab, BoolOp::Intersect).unwrap(), BoolOp::Subtract).unwrap();
    let crests =
      |s: &Shape| s.faces().unwrap().iter().filter(|f| f.kind == SurfaceKind::Cylinder && f.radius > 4.9).count();
    let before = crests(&cut);
    assert!(before > 3, "a strip per turn before the merge");
    let merged = cut.merge_cylinder_strips(DVec3::ZERO, DVec3::Z, 5.0, (0.0, 12.0)).unwrap();
    assert_eq!(crests(&merged), 1);
    assert!(merged.is_valid());
    let (a, b) = (cut.mass().unwrap(), merged.mass().unwrap());
    assert!((a.volume - b.volume).abs() < 1e-3 * a.volume, "{a:?} {b:?}");
    assert!((a.area - b.area).abs() < 1e-3 * a.area, "{a:?} {b:?}");
    assert!((a.center_of_mass - b.center_of_mass).length() < 1e-3, "{a:?} {b:?}");
    // the axis may point either way
    let down = cut.merge_cylinder_strips(DVec3::new(0.0, 0.0, 12.0), -DVec3::Z, 5.0, (0.0, 12.0)).unwrap();
    assert_eq!(crests(&down), 1);
    // faces on another radius, or outside the span, are not touched
    assert_eq!(crests(&cut.merge_cylinder_strips(DVec3::ZERO, DVec3::Z, 4.0, (0.0, 12.0)).unwrap()), before);
    assert!(crests(&cut.merge_cylinder_strips(DVec3::ZERO, DVec3::Z, 5.0, (0.0, 6.0)).unwrap()) > 1);
    // a plain solid comes back as it was
    assert_eq!(rod.merge_cylinder_strips(DVec3::ZERO, DVec3::Z, 5.0, (0.0, 12.0)).unwrap().faces().unwrap().len(), 3);
  }
}
