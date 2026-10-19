//! Transparent preview bodies: while a command dialog shows its result live in the viewport, the
//! bodies it changed are also drawn as they were before, as see-through ghosts, as in Fusion.
//!
//! The ghosts are painted with egui over the rendered image, so they work with any renderer.

use clf_engine::Session;
use clf_engine::kernel::TriMesh;
use clf_engine::kernel::glam::DVec3;
use clf_engine::model::BodyId;
use egui::{Color32, Pos2};

use crate::App;

/// Ghost colour (before shading) and opacity.
const GHOST: [u8; 3] = [70, 120, 200];
const ALPHA: f32 = 0.16;

/// The bodies as a dialog found them, and which of them its preview has changed.
#[derive(Default)]
pub struct Ghost {
  before: Vec<(BodyId, TriMesh)>,
  /// The revision `changed` was worked out for.
  at: Option<u64>,
  changed: Vec<usize>,
}

impl Ghost {
  /// Remembers the shown bodies as they are, before a dialog changes them.
  pub fn capture(&mut self, s: &Session) {
    self.before =
      s.model.bodies.iter().filter(|b| s.body_visible(b.id)).filter_map(|b| Some((b.id, b.mesh()?.clone()))).collect();
    self.at = None;
    self.changed.clear();
  }

  pub fn clear(&mut self) {
    if !self.before.is_empty() || self.at.is_some() {
      *self = Ghost::default();
    }
  }

  /// The remembered bodies that differ from the model now (moved, reshaped or gone).
  pub fn changed(&mut self, s: &Session) -> Vec<&(BodyId, TriMesh)> {
    if self.at != Some(s.revision) {
      self.changed = changed_bodies(&self.before, s);
      self.at = Some(s.revision);
    }
    self.changed.iter().filter_map(|i| self.before.get(*i)).collect()
  }
}

/// Indices of the `before` bodies whose meshes the model no longer has.
pub fn changed_bodies(before: &[(BodyId, TriMesh)], s: &Session) -> Vec<usize> {
  before
    .iter()
    .enumerate()
    .filter(|(_, (id, m))| match s.model.body(*id).and_then(|b| b.mesh()) {
      Some(now) => now.positions != m.positions || now.indices != m.indices,
      None => true,
    })
    .map(|(i, _)| i)
    .collect()
}

/// A mesh's triangles as an egui mesh in screen space, shaded by how squarely each faces the
/// viewer (`toward`: unit vector to the eye); triangles off screen are left out.
pub fn screen_mesh(m: &TriMesh, toward: DVec3, project: &dyn Fn(DVec3) -> Option<Pos2>) -> egui::Mesh {
  let mut out = egui::Mesh::default();
  let pts: Vec<Option<Pos2>> =
    m.positions.iter().map(|p| project(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))).collect();
  for tri in m.indices.chunks_exact(3) {
    let ix = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
    let (Some(Some(a)), Some(Some(b)), Some(Some(c))) = (pts.get(ix[0]), pts.get(ix[1]), pts.get(ix[2])) else {
      continue;
    };
    let n: DVec3 =
      ix.iter().filter_map(|i| m.normals.get(*i)).map(|n| DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)).sum();
    let lit = 0.55 + 0.45 * n.try_normalize().map_or(0.5, |n| n.dot(toward).abs()) as f32;
    let col =
      Color32::from_rgb((GHOST[0] as f32 * lit) as u8, (GHOST[1] as f32 * lit) as u8, (GHOST[2] as f32 * lit) as u8)
        .gamma_multiply(ALPHA);
    let i = out.vertices.len() as u32;
    for p in [a, b, c] {
      out.colored_vertex(*p, col);
    }
    out.add_triangle(i, i + 1, i + 2);
  }
  out
}

/// Paints the ghosts of what the open dialog's preview has changed. Drops the remembered bodies
/// once no dialog is open.
pub(crate) fn overlay(app: &mut App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let Some(d) = app.dialog.as_ref() else {
    app.ghost.clear();
    return;
  };
  if !d.preview_applied() || d.editing.is_some() {
    return;
  }
  let (_, _, back) = app.cam.basis();
  for (_, m) in app.ghost.changed(&app.s) {
    painter.add(screen_mesh(m, back, project));
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn ghosts_are_the_bodies_a_preview_changed() {
    let mut s = Session::default();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    s.execute("box", &json!({"width": 5, "depth": 5, "height": 5, "at": [40, 0]})).unwrap();
    let mut g = Ghost::default();
    g.capture(&s);
    assert!(g.changed(&s).is_empty());
    let second = s.model.bodies[1].id;
    s.execute("move", &json!({"bodies": [second.0], "translate": [0, 0, 7]})).unwrap();
    let changed = g.changed(&s);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].0, second);
    // the ghost is the body where it was: its mesh, flat on the screen
    let m = changed[0].1.clone();
    let flat = screen_mesh(&m, DVec3::Z, &|p| Some(Pos2::new(p.x as f32, -p.y as f32)));
    assert_eq!(flat.indices.len(), m.indices.len());
    assert!(m.positions.iter().all(|p| p[2] <= 5.0 + 1e-6));
    // triangles that leave the screen are dropped
    let none = screen_mesh(&m, DVec3::Z, &|_| None);
    assert!(none.indices.is_empty());
    g.clear();
    assert!(g.changed(&s).is_empty());
  }
}
