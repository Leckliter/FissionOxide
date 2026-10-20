//! 3D curves: where shapes cross, as wires (Fusion's Intersection Curve pushes sketch curves
//! along their normal and keeps where they cross faces), and where a wire starts. The C++ side is
//! `cpp/curves3d.cpp`.

use glam::DVec3;

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

impl Shape {
  /// The curves where this shape and `other` cross, chained into wires: edges whose ends meet
  /// within `tolerance` join into one wire (a section split at a face's seam comes back whole).
  pub fn section_wires(&self, other: &Shape, tolerance: f64) -> Result<Vec<Shape>> {
    if !(tolerance > 0.0 && tolerance.is_finite()) {
      return Err(KernelError::Input("the tolerance must be positive".into()));
    }
    let c = Shape::wrap(sys::section_wires(self.get()?, other.get()?, tolerance)?)?;
    c.wires()
  }

  /// The edges of `wires` chained end to end into as few wires as they make: ends that meet
  /// within `tolerance` join, so curves sectioned one at a time come back as connected paths.
  pub fn chain_wires(wires: &[Shape], tolerance: f64) -> Result<Vec<Shape>> {
    if !(tolerance > 0.0 && tolerance.is_finite()) {
      return Err(KernelError::Input("the tolerance must be positive".into()));
    }
    let c = Shape::compound(wires)?;
    Shape::wrap(sys::chain_wires(c.get()?, tolerance)?)?.wires()
  }

  /// This wire split at its sharp corners (where its direction turns by more than `angle`
  /// radians) into smooth runs, in order along it. A closed wire with corners splits into runs
  /// that each start and end at a corner; a wire without corners comes back whole.
  pub fn split_at_corners(&self, angle: f64) -> Result<Vec<Shape>> {
    Shape::wrap(sys::split_at_corners(self.get()?, angle)?)?.wires()
  }

  /// Where this wire turns sharply: its direction turns by more than `angle` radians.
  pub fn wire_corners(&self, angle: f64) -> Result<Vec<DVec3>> {
    let v = sys::wire_corners(self.get()?, angle)?;
    Ok(v.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect())
  }

  /// The wires in this shape, in order.
  pub fn wires(&self) -> Result<Vec<Shape>> {
    let s = self.get()?;
    (0..sys::wire_count(s)).map(|i| Shape::wrap(sys::wire_at(s, i)?)).collect()
  }

  /// Where this wire starts and the unit direction it leaves in.
  pub fn wire_start(&self) -> Result<(DVec3, DVec3)> {
    let v = sys::wire_start(self.get()?)?;
    if v.len() != 6 {
      return Err(KernelError::Kernel("the wire's start could not be found".into()));
    }
    Ok((DVec3::new(v[0], v[1], v[2]), DVec3::new(v[3], v[4], v[5])))
  }

  /// A closed wire restarted at the middle (by length) of its first edge, whose halves become
  /// its first and last edges: Fusion's pipes start there on closed paths. An open wire comes
  /// back unchanged.
  pub fn wire_from_middle(&self) -> Result<Shape> {
    Shape::wrap(sys::wire_from_middle(self.get()?)?)
  }
}
