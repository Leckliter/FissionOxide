//! Fusion's surface Patch with a continuity per boundary edge and interior rails, its Ruled surface
//! grown from one edge, and its surface Sweep and Loft (open surfaces from curves). The C++ side
//! is `cpp/surface3.cpp`.

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

/// How a patch meets the face beside one of its boundary edges (Fusion's continuity column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Continuity {
  /// G0: the patch only touches the edge.
  #[default]
  Connected,
  /// G1: the patch leaves the edge along the face.
  Tangent,
  /// G2: the patch's curvature matches the face's too.
  Curvature,
}

impl Shape {
  /// A patch over the closed boundary `edges`: each edge with its continuity and, unless it is
  /// [`Continuity::Connected`], the face beside it; the patch also passes through the `interior`
  /// curves (rails).
  pub fn patch_with(edges: &[(Shape, Continuity, Option<Shape>)], interior: &[Shape]) -> Result<Shape> {
    if edges.is_empty() {
      return Err(KernelError::Input("pick the boundary edges".into()));
    }
    let boundary = Shape::compound(edges.iter().map(|e| &e.0))?;
    let mut orders = Vec::new();
    let mut faces = Vec::new();
    for (_, c, face) in edges {
      match (c, face) {
        (Continuity::Connected, _) | (_, None) => orders.push(0),
        (c, Some(f)) => {
          orders.push(if *c == Continuity::Tangent { 1 } else { 2 });
          faces.push(f);
        }
      }
    }
    let supports = Shape::compound(faces)?;
    let interior = Shape::compound(interior)?;
    Shape::wrap(sys::patch_with(boundary.get()?, supports.get()?, &orders, interior.get()?)?)
  }

  /// A ruled surface from this edge `distance` out of `face` (an adjacent face): along the face's
  /// normal, or with `tangent` along the face itself away from it; `angle` (radians) turns it
  /// about the edge.
  pub fn ruled_from_edge(&self, face: &Shape, distance: f64, angle: f64, tangent: bool) -> Result<Shape> {
    if !distance.is_finite() || !angle.is_finite() {
      return Err(KernelError::Input("the distance and angle must be finite".into()));
    }
    Shape::wrap(sys::ruled_from_edge(self.get()?, face.get()?, distance, angle, tangent)?)
  }

  /// An open surface: this curve (a wire, edge or compound of edges) swept along `path` (a wire),
  /// kept square to it, or with `parallel` keeping its own orientation.
  pub fn sweep_open(&self, path: &Shape, parallel: bool) -> Result<Shape> {
    Shape::wrap(sys::sweep_open(self.get()?, path.get()?, parallel)?)
  }

  /// An open surface through the section curves (wires, edges or compounds of edges) in order;
  /// `closed` loops back to the first.
  pub fn loft_open(sections: &[Shape], closed: bool) -> Result<Shape> {
    if sections.len() < 2 {
      return Err(KernelError::Input("a loft needs at least two profiles".into()));
    }
    let c = Shape::compound(sections)?;
    Shape::wrap(sys::loft_open(c.get()?, closed)?)
  }
}
