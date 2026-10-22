//! Regeneration of MESH > Tessellate, Repair, Generate Face Groups and Combine Face Groups.

use fox_kernel::glam::DVec3;
use fox_kernel::{Shape, TriMesh};

use super::{Ctx, FResult, ke};
use crate::meshprep;
use crate::{BodyId, FeatureId, FeatureKind};

/// The triangles of a body's mesh with shared vertices, and each triangle's face.
pub(crate) fn mesh_triangles(m: &TriMesh) -> (Vec<DVec3>, Vec<u32>, Vec<u32>) {
  let mut index: std::collections::HashMap<[i64; 3], u32> = std::collections::HashMap::new();
  let mut pos = Vec::new();
  let q = |v: f32| (v as f64 * 1e5).round() as i64;
  let map: Vec<u32> = m
    .positions
    .iter()
    .map(|p| {
      *index.entry([q(p[0]), q(p[1]), q(p[2])]).or_insert_with(|| {
        pos.push(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
        (pos.len() - 1) as u32
      })
    })
    .collect();
  let tris = m.indices.iter().map(|&i| map.get(i as usize).copied().unwrap_or(0)).collect();
  (pos, tris, m.tri_face.clone())
}

fn shape_of(pos: &[DVec3], tris: &[u32]) -> Result<Shape, String> {
  let pos32: Vec<[f32; 3]> = pos.iter().map(|p| p.as_vec3().to_array()).collect();
  Shape::from_mesh(&pos32, tris, false).map_err(ke)
}

impl Ctx<'_> {
  fn unitless(&self, e: &str) -> Result<f64, String> {
    fox_expr::eval(e, fox_expr::Kind::Unitless, self.params).map_err(|err| format!("'{e}': {err}"))
  }

  pub(super) fn apply_mesh_prep(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::Tessellate {
        body, refinement, surface_deviation, normal_deviation, max_edge, aspect_ratio, ..
      } => {
        let bi = self.body_index(*body)?;
        let diag = self.state.bodies[bi].diagonal();
        let mut r = meshprep::refinement(refinement, diag);
        if refinement == "custom" {
          if !surface_deviation.is_empty() {
            r.surface_deviation = self.len(surface_deviation)?;
          }
          if !normal_deviation.is_empty() {
            r.normal_deviation = self.unitless(normal_deviation)?;
          }
          if !max_edge.is_empty() {
            r.max_edge = self.len(max_edge)?;
          }
          if !aspect_ratio.is_empty() {
            r.aspect_ratio = self.unitless(aspect_ratio)?;
          }
        }
        let shape = &self.state.bodies[bi].shape;
        let m = shape.tessellate_with(r.surface_deviation.max(1e-4), r.normal_deviation.to_radians()).map_err(ke)?;
        let (pos, tris, _) = mesh_triangles(&m);
        let (pos, tris) = meshprep::split_long_edges(&pos, &tris, r.max_edge, 400_000);
        let shape = shape_of(&pos, &tris)?;
        self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        self.state.bodies[bi].kind = super::BodyKind::Mesh;
        Ok(None)
      }
      FeatureKind::MeshRepair { bodies, repair } => {
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let m = self.state.bodies[bi].mesh().cloned().ok_or("the body has no mesh")?;
          let (pos, tris, _) = mesh_triangles(&m);
          let (pos, tris) = meshprep::repair(&pos, &tris, *repair);
          if tris.is_empty() {
            return Err("nothing is left of the mesh after the repair".into());
          }
          let shape = shape_of(&pos, &tris)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::GenerateFaceGroups { bodies, accurate, angle, min_size } => {
        let angle = self.angle(angle)?.to_degrees();
        let min_size = self.unitless(min_size)?;
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let m = self.state.bodies[bi].mesh().cloned().ok_or("the body has no mesh")?;
          let (pos, tris, tri_face) = mesh_triangles(&m);
          let groups = if *accurate {
            meshprep::face_groups(&pos, &tris, angle * 0.75, min_size)
          } else {
            meshprep::face_groups(&pos, &tris, angle, min_size)
          };
          let faces = self.state.bodies[bi].faces().len();
          self.state.face_groups.insert(*b, per_face(&groups, &tri_face, faces));
        }
        Ok(None)
      }
      FeatureKind::CombineFaceGroups { faces } => {
        if faces.is_empty() {
          return Err("select two or more adjacent face groups".into());
        }
        let mut by_body: Vec<(BodyId, Vec<u32>)> = Vec::new();
        for fr in faces.iter() {
          let bi = self.body_index(fr.body)?;
          let face = self.state.bodies[bi].find_face(fr).ok_or("a picked face is gone")?;
          match by_body.iter_mut().find(|(b, _)| *b == fr.body) {
            Some((_, v)) => v.push(face),
            None => by_body.push((fr.body, vec![face])),
          }
        }
        for (b, picked) in by_body {
          let n = self.body_index(b).map(|bi| self.state.bodies[bi].faces().len())?;
          // a body without groups yet: each face is its own
          let groups = self.state.face_groups.entry(b).or_insert_with(|| (0..n as u32).collect());
          let picked: Vec<u32> = picked.iter().filter_map(|f| groups.get(*f as usize - 1).copied()).collect();
          let mut distinct = picked.clone();
          distinct.sort_unstable();
          distinct.dedup();
          if distinct.len() < 2 {
            return Err("select two or more adjacent face groups".into());
          }
          meshprep::combine_groups(groups, &distinct);
        }
        Ok(None)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }
}

/// A group per face (face ids from 1) from a group per triangle: the group of each face's first
/// triangle.
fn per_face(groups: &[u32], tri_face: &[u32], faces: usize) -> Vec<u32> {
  let mut out = vec![u32::MAX; faces];
  for (t, f) in tri_face.iter().enumerate() {
    if let (Some(slot), Some(g)) = (out.get_mut((*f as usize).wrapping_sub(1)), groups.get(t))
      && *slot == u32::MAX
    {
      *slot = *g;
    }
  }
  // faces with no triangles get groups of their own
  let first = groups.iter().max().map_or(0, |m| m + 1);
  for (next, g) in (first..).zip(out.iter_mut().filter(|g| **g == u32::MAX)) {
    *g = next;
  }
  out
}
