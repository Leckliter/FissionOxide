//! Modification history. A shape's faces and edges can carry labels (numbers the model gives
//! them); the kernel calls that build a shape from labelled ones follow OpenCASCADE's own history
//! (`BRepBuilderAPI_MakeShape::Modified`/`Generated`/`IsDeleted`, the Boolean and
//! UnifySameDomain `BRepTools_History`) and say, for each face of the result, which labelled
//! faces it was modified from and which labelled faces or edges it was generated from. The
//! C++ side is `cpp/history.cpp`.
//!
//! Calls that keep no history give a shape without labels; [`Shape::same_faces`] then pairs
//! faces that are the same face, or lie on the same surface, as before.

use crate::ffi::ffi as sys;
use crate::{Result, Shape};

/// For each face of a shape (index `id - 1`), the labels it traces back to: `(label, generated)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FaceLabels {
  pub faces: Vec<Vec<(u64, bool)>>,
}

impl Shape {
  /// Labels this shape's faces, as `(1-based face id, label)` pairs (a face may have several), and
  /// its edges, by 1-based index (`edges[id - 1]`, 0 for none). With no edge labels given, each
  /// edge between two labelled faces is labelled from their smallest labels. Later kernel calls
  /// can then say what became of them. Replaces any history the shape carried.
  pub fn set_labels(&mut self, faces: &[(u32, u64)], edges: &[u64]) -> Result<()> {
    if self.inner.is_null() {
      return Err(crate::KernelError::Empty);
    }
    let (ids, labels): (Vec<u32>, Vec<u64>) = faces.iter().copied().unzip();
    sys::set_labels(self.inner.pin_mut(), &ids, &labels, edges)?;
    Ok(())
  }

  /// [`Shape::set_labels`], on a copy.
  pub fn labelled(&self, faces: &[(u32, u64)], edges: &[u64]) -> Result<Shape> {
    let mut s = self.clone();
    s.set_labels(faces, edges)?;
    Ok(s)
  }

  /// Whether this shape carries history back to labelled faces or edges.
  pub fn has_labels(&self) -> bool {
    self.get().is_ok_and(sys::has_labels)
  }

  /// What each face of this shape came from, among the labelled shapes it was built from.
  pub fn face_labels(&self) -> Result<FaceLabels> {
    let d = sys::face_labels(self.get()?)?;
    let faces = d
      .offsets
      .windows(2)
      .map(|w| {
        let (a, b) = (w[0] as usize, w[1] as usize);
        (a..b.min(d.labels.len())).map(|i| (d.labels[i], d.generated.get(i).is_some_and(|g| *g != 0))).collect()
      })
      .collect();
    Ok(FaceLabels { faces })
  }

  /// Per face of this shape (index `id - 1`), the 1-based id of the face of `before` it is: the
  /// same face, or the first one on the same surface; 0 for none.
  pub fn same_faces(&self, before: &Shape) -> Result<Vec<u32>> {
    Ok(sys::same_faces(self.get()?, before.get()?)?)
  }
}
