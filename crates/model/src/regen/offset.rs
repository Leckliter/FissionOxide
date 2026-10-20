//! Offset Face (press-pull on faces) as Fusion does it, behind
//! [`OFFSET_FACES_EXTEND`](fox_sketch::behaviour::OFFSET_FACES_EXTEND).

use fox_sketch::behaviour::OFFSET_FACES_EXTEND;

use super::{Body, Ctx};

impl Ctx<'_> {
  /// `body` with its faces `picked` moved `dist` along their normals and every neighbour grown or
  /// trimmed to meet them (a slanted side runs on along its slope), as Fusion's Offset Face does.
  /// `None` for designs made before [`OFFSET_FACES_EXTEND`], or where the kernel can't offset the
  /// faces that way (a face pushed past a neighbour, say): the caller then extrudes or thickens each
  /// face and adds or cuts it, as before.
  pub(super) fn offset_faces_extending(&self, body: &Body, picked: &[u32], dist: f64) -> Option<fox_kernel::Shape> {
    if !self.behaviours.has(OFFSET_FACES_EXTEND) || picked.is_empty() {
      return None;
    }
    let mut ids = picked.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let s = body.shape.offset_faces(&ids, dist).ok()?;
    (s.solid_count() > 0).then_some(s)
  }
}
