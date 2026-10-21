//! Form bodies, as Fusion's Form workspace: a polygon control cage sculpted with primitives and
//! edits (Edit Form, insert/delete edge, insert point, subdivide, extrude, bridge, fill hole,
//! crease, symmetry, pull, Make Uniform), whose smooth body is the cage's Catmull-Clark limit
//! surface. [`subd::patches`] turns that surface into bicubic B-spline patches that sew into an
//! exact B-rep body.
//!
//! Pure geometry: no kernel, so the model, the engine and the UI share it.

pub mod bspline;
pub mod edit;
pub mod mesh;
pub mod prim;
pub mod subd;
pub mod symmetry;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use bspline::Patch;
pub use mesh::{Cage, Display, MAX_FACES};
pub use symmetry::{SymPlane, Symmetry};

/// Picked cage elements: vertices, edges (two vertices each) and faces (indices).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Selection {
  #[serde(default)]
  pub vertices: Vec<u32>,
  #[serde(default)]
  pub edges: Vec<[u32; 2]>,
  #[serde(default)]
  pub faces: Vec<u32>,
}

impl Selection {
  pub fn is_empty(&self) -> bool {
    self.vertices.is_empty() && self.edges.is_empty() && self.faces.is_empty()
  }

  /// Sorts and removes duplicates (edges lower vertex first).
  pub fn normalize(&mut self) {
    for e in &mut self.edges {
      if e[0] > e[1] {
        e.swap(0, 1);
      }
    }
    for v in [&mut self.vertices, &mut self.faces] {
      v.sort_unstable();
      v.dedup();
    }
    self.edges.sort_unstable();
    self.edges.dedup();
  }

  /// Every vertex the selection touches: its vertices, edge ends and face corners.
  pub fn vertex_set(&self, cage: &Cage) -> BTreeSet<u32> {
    let mut out: BTreeSet<u32> = self.vertices.iter().copied().collect();
    out.extend(self.edges.iter().flatten().copied());
    for &f in &self.faces {
      if let Some(face) = cage.faces.get(f as usize) {
        out.extend(face.iter().copied());
      }
    }
    out.retain(|v| (*v as usize) < cage.vertices.len());
    out
  }
}
