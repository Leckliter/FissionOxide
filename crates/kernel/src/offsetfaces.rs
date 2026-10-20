//! Offset Face as Fusion does it: chosen faces move along their normals and their neighbours
//! extend or trim to meet them. The C++ side is `cpp/offsetfaces.cpp`.

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

impl Shape {
  /// This solid with `faces` (1-based face-map indices) moved `distance` along their outward
  /// normals (negative: into the material). Every other face keeps its surface and grows or
  /// shrinks to meet them, so a slanted neighbour runs on along its slope and a curved face is
  /// offset (a hole's wall shrinks or grows the hole).
  pub fn offset_faces(&self, faces: &[u32], distance: f64) -> Result<Shape> {
    if !distance.is_finite() || distance.abs() < 1e-9 {
      return Err(KernelError::Input("the offset distance must not be zero".into()));
    }
    if faces.is_empty() {
      return Err(KernelError::Input("pick the faces to offset".into()));
    }
    Shape::wrap(sys::offset_faces(self.get()?, faces, distance)?)
  }
}
