//! A body's silhouette on a plane, for projecting a whole body into a sketch. The C++ side is
//! `cpp/silhouette.cpp`.

use crate::ffi::ffi as sys;
use crate::{Plane, Result, Shape};

impl Shape {
  /// The outline of this shape's shadow cast along `plane`'s normal, as edges lying on the plane:
  /// the outer boundary and any holes right through, with outlines of curved faces (a cylinder
  /// lying on its side shows as a rectangle). Edges that land on each other appear once.
  pub fn silhouette(&self, plane: &Plane) -> Result<Shape> {
    Shape::wrap(sys::silhouette(self.get()?, &plane.data())?)
  }
}
