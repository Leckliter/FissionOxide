//! Surface workspace operations (C++ in `cpp/surface2.cpp`): extend, untrim, trim sides and
//! variable offsets.

use glam::DVec3;

use crate::{KernelError, Result, Shape, sys};

/// How an extended surface edge continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extension {
  /// The face's own surface carries on (a plane stays a plane, a cylinder a cylinder, a B-spline
  /// continues with matching curvature). Edges that aren't along a parameter line of their
  /// surface extend tangentially.
  Natural,
  /// A ruled strip leaving the edge tangent to the surface.
  Tangent,
  /// A ruled strip leaving the edge along the surface normal.
  Perpendicular,
}

/// Which boundary loops Untrim removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Untrim {
  /// Holes and the trimmed outer boundary: the whole natural surface.
  All,
  /// Only the holes.
  Inner,
  /// Only the outer boundary (holes stay).
  Outer,
}

impl Shape {
  /// Extends free (boundary) edges (1-based edge ids) by `distance`, sewing the extensions on.
  pub fn extend_edges(&self, edges: &[u32], distance: f64, how: Extension) -> Result<Shape> {
    self.extend_edges_with(edges, distance, how, false)
  }

  /// [`Shape::extend_edges`], with `close_corners`: where two extended edges meet at a corner,
  /// the gap between their extensions is closed with a patch (on a plane the extensions and the
  /// corner make one bigger face).
  pub fn extend_edges_with(&self, edges: &[u32], distance: f64, how: Extension, close_corners: bool) -> Result<Shape> {
    if !(distance > 0.0 && distance.is_finite()) {
      return Err(KernelError::Input("the extension distance must be positive".into()));
    }
    if edges.is_empty() {
      return Err(KernelError::Input("no edges to extend".into()));
    }
    let mode = match how {
      Extension::Natural => 0,
      Extension::Tangent => 1,
      Extension::Perpendicular => 2,
    };
    Shape::wrap(sys::extend_edges(self.get()?, edges, distance, mode, close_corners)?)
  }

  /// Untrims faces (1-based face ids): removes their holes and/or restores the untrimmed outer
  /// boundary of their surface (a plane's or extrusion's open directions keep the face's extent).
  pub fn untrim(&self, faces: &[u32], loops: Untrim) -> Result<Shape> {
    self.untrim_extended(faces, loops, 0.0)
  }

  /// [`Shape::untrim`], then the restored outer boundary grown outward by `extension` (a length
  /// along the surface; sides where the surface closes on itself stay). Holes-only untrims keep
  /// their boundary, so they take no extension.
  pub fn untrim_extended(&self, faces: &[u32], loops: Untrim, extension: f64) -> Result<Shape> {
    if !(extension >= 0.0 && extension.is_finite()) {
      return Err(KernelError::Input("the untrim extension can't be negative".into()));
    }
    if extension > 0.0 && loops == Untrim::Inner {
      return Err(KernelError::Input("an extension grows the outer boundary: untrim all or external loops".into()));
    }
    let mode = match loops {
      Untrim::All => 0,
      Untrim::Inner => 1,
      Untrim::Outer => 2,
    };
    Shape::wrap(sys::untrim_faces(self.get()?, faces, mode, extension)?)
  }

  /// Which side of this trimming tool `p` is on. A solid gives +1 inside, -1 outside and 0 on
  /// its skin; surfaces give the signed distance along the nearest face's normal.
  pub fn trim_side(&self, p: DVec3) -> Result<f64> {
    Ok(sys::classify_point(self.get()?, p.x, p.y, p.z)?)
  }

  /// Offsets this surface (or a solid's skin) by `distance`, with some faces (1-based ids) at
  /// distances of their own.
  pub fn offset_variable(&self, distance: f64, faces: &[(u32, f64)]) -> Result<Shape> {
    if !distance.is_finite() || faces.iter().any(|f| !f.1.is_finite()) {
      return Err(KernelError::Input("offset distances must be finite".into()));
    }
    let ids: Vec<u32> = faces.iter().map(|f| f.0).collect();
    let dists: Vec<f64> = faces.iter().map(|f| f.1).collect();
    Shape::wrap(sys::offset_variable(self.get()?, distance, &ids, &dists)?)
  }

  /// This face or connected shell offset by `d` as one piece, the offsets of faces meeting at an
  /// angle extended until they meet (`round`: joined by an arc about the edge instead), so corners
  /// stay closed.
  pub fn offset_joined(&self, d: f64, round: bool) -> Result<Shape> {
    if !d.is_finite() {
      return Err(KernelError::Input("offset distances must be finite".into()));
    }
    Shape::wrap(sys::offset_joined(self.get()?, d, round, false)?)
  }

  /// This face or connected shell thickened by `d` (positive = along the face normals) as one
  /// solid, with sharp (or `round`) corners where faces meet at an angle. Fails, rather than
  /// returning something partial, when OCCT can't build a single valid solid.
  pub fn thicken_joined(&self, d: f64, round: bool) -> Result<Shape> {
    if !d.is_finite() {
      return Err(KernelError::Input("the thickness must be finite".into()));
    }
    let s = Shape::wrap(sys::offset_joined(self.get()?, d, round, true)?)?;
    if s.solid_count() != 1 {
      return Err(KernelError::Input("thickening the faces together made no single solid".into()));
    }
    // offsetting against a face's normal can leave the solid inside out
    if s.mass().is_ok_and(|m| m.volume < 0.0) { s.reversed() } else { Ok(s) }
  }
}
