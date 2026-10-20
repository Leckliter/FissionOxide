//! Project To Surface: sketch curves draped onto faces as 3D wires, along a direction or to the
//! closest point (Fusion's Sketch > Project To Surface), and round pipes along such wires, which
//! are often closed loops. The C++ side is `cpp/project.cpp`.

use glam::DVec3;

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

/// How curves are carried onto the faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection3d {
  /// Each point drops onto the nearest point of the faces.
  ClosestPoint,
  /// Each point moves along this direction (either way) until it meets a face.
  Along(DVec3),
}

/// Where a path wire starts and ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathEnds {
  pub start: DVec3,
  /// Unit tangent at the start, in the direction of travel.
  pub tangent: DVec3,
  pub end: DVec3,
  pub closed: bool,
}

impl Shape {
  /// The curves (edges or wires, chained end to end) projected onto the faces: one curve per
  /// connected run (a periodic B-spline when it closes), starting where the source's start lands.
  /// Along a direction a run goes the way the swept source crosses the faces, as Fusion's do;
  /// `sketch_normal` is the normal of the sketch the curves come from.
  pub fn project_to_faces(
    curves: &[Shape],
    faces: &[Shape],
    how: Projection3d,
    sketch_normal: DVec3,
  ) -> Result<Vec<Shape>> {
    if curves.is_empty() {
      return Err(KernelError::Input("there are no curves to project".into()));
    }
    if faces.is_empty() {
      return Err(KernelError::Input("there are no faces to project onto".into()));
    }
    let dir = match how {
      Projection3d::ClosestPoint => Vec::new(),
      Projection3d::Along(d) => {
        let d = d.try_normalize().ok_or_else(|| KernelError::Input("the projection direction has no length".into()))?;
        [d.to_array(), sketch_normal.normalize_or_zero().to_array()].concat()
      }
    };
    let (c, f) = (Shape::compound(curves)?, Shape::compound(faces)?);
    Shape::wrap(sys::project_curves(c.get()?, f.get()?, &dir)?)?.children()
  }

  /// A round pipe of `radius` along this path wire (open or closed), the circle square to the
  /// path at its start; hollow inside `inner` when that's above zero.
  pub fn round_pipe(&self, radius: f64, inner: f64) -> Result<Shape> {
    if !(radius.is_finite() && radius > 0.0 && inner.is_finite()) {
      return Err(KernelError::Input("the diameter must be positive".into()));
    }
    Shape::wrap(sys::round_pipe(self.get()?, radius, inner.max(0.0))?)
  }

  /// This shape with its faces on periodic free-form surfaces (a tube's) split where their
  /// parameters run across the surface's seam (the band a cut leaves of a pipe, say), as a face
  /// of Fusion's ends at the seam. Unchanged when there are none, or the split doesn't hold.
  pub fn split_seams(&self) -> Result<Shape> {
    Shape::wrap(sys::split_seams(self.get()?)?)
  }

  /// Where this path wire (or edge) starts and ends.
  pub fn path_ends(&self) -> Result<PathEnds> {
    let v = sys::wire_ends(self.get()?)?;
    if v.len() < 10 {
      return Err(KernelError::Kernel("the path has no ends".into()));
    }
    Ok(PathEnds {
      start: DVec3::new(v[0], v[1], v[2]),
      tangent: DVec3::new(v[3], v[4], v[5]),
      end: DVec3::new(v[6], v[7], v[8]),
      closed: v[9] > 0.5,
    })
  }

  /// The shapes directly inside this one (the members of a compound).
  pub fn children(&self) -> Result<Vec<Shape>> {
    let s = self.get()?;
    (0..sys::child_count(s)).map(|i| Shape::wrap(sys::child_at(s, i)?)).collect()
  }
}
