//! Regeneration of the MESH > MODIFY commands that rebuild a mesh body's triangles: Remesh,
//! Shell, Combine and Separate (Reduce, Smooth, Convert Mesh and Plane Cut are in `mesh`).
//!
//! They read and write a body's triangles through `mesh::triangles` and `mesh::shape`, and the
//! bodies keep their kind ([`super::BodyKind`]).

use fox_kernel::glam::DVec3;
use fox_kernel::{BoolOp, Shape};

use super::mesh::{shape as mesh_shape_merged, triangles};
use super::{Ctx, FResult, ke};
use crate::construct::MeshCombineOp;
use crate::meshops;
use crate::{BodyId, FeatureId, FeatureKind};

/// Triangles as a mesh body's geometry.
fn mesh_shape(pos: &[DVec3], tris: &[u32]) -> Result<Shape, String> {
  mesh_shape_merged(pos, tris, false)
}

impl Ctx<'_> {
  /// The welded triangles of body `bi`.
  fn mesh_of(&self, bi: usize) -> Result<(Vec<DVec3>, Vec<u32>), String> {
    let (pos, tris) = triangles(&self.state.bodies[bi])?;
    if tris.is_empty() {
      return Err("the body has no triangles".into());
    }
    Ok((pos, tris))
  }

  pub(super) fn apply_mesh_edit(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::MeshRemesh { bodies, adaptive, edge_length, density, shape, sharp, boundaries } => {
        let unitless =
          |e: &str| fox_expr::eval(e, fox_expr::Kind::Unitless, self.params).map_err(|err| format!("'{e}': {err}"));
        let shape = unitless(shape)?;
        let density = unitless(density)?;
        let edge = edge_length.as_ref().map(|e| self.len(e)).transpose()?;
        if edge.is_some_and(|e| e <= 0.0) {
          return Err("the edge length must be positive".into());
        }
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let (pos, tris) = self.mesh_of(bi)?;
          let target = edge.unwrap_or_else(|| meshops::density_edge(self.state.bodies[bi].diagonal(), density));
          let opts = meshops::Remesh { target, adaptive: *adaptive, shape, sharp: *sharp, boundaries: *boundaries };
          let (pos, tris) = meshops::remesh(&pos, &tris, &opts);
          let s = mesh_shape(&pos, &tris)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
        }
        Ok(None)
      }
      FeatureKind::MeshShell { bodies, thickness } => {
        let t = self.len(thickness)?;
        if t.abs() < 1e-9 {
          return Err("give the shell a thickness".into());
        }
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let (pos, tris) = self.mesh_of(bi)?;
          let s = if meshops::boundary_edges(&tris).is_empty() {
            // closed: the body less its copy offset inward (or a copy offset outward less the body)
            let moved = mesh_shape(&meshops::offset(&pos, &tris, -t), &tris)?;
            let body = self.state.bodies[bi].shape.clone();
            let (outer, inner) = if t > 0.0 { (body, moved) } else { (moved, body) };
            outer.boolean(&inner, BoolOp::Subtract).map_err(ke)?
          } else {
            let (p, tr) = meshops::shell_open(&pos, &tris, t);
            mesh_shape(&p, &tr)?
          };
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
        }
        Ok(None)
      }
      FeatureKind::MeshCombine { target, tools, operation, keep_tools } => {
        let target = *target;
        let ti = self.body_index(target)?;
        let tools: Vec<BodyId> = tools.iter().copied().filter(|t| *t != target).collect();
        if tools.is_empty() {
          return Err("select the tool bodies".into());
        }
        let mut shapes = Vec::new();
        for t in &tools {
          shapes.push(self.state.bodies[self.body_index(*t)?].shape.clone());
        }
        let mut shape = self.state.bodies[ti].shape.clone();
        match operation {
          MeshCombineOp::Merge => {
            let all: Vec<&Shape> = std::iter::once(&shape).chain(&shapes).collect();
            shape = Shape::compound(all).map_err(ke)?;
          }
          MeshCombineOp::Intersect => {
            // the part of the target inside any tool, as the solid Combine
            let mut union = shapes[0].clone();
            for s in &shapes[1..] {
              union = union.boolean(s, BoolOp::Union).map_err(ke)?;
            }
            shape = shape.boolean(&union, BoolOp::Intersect).map_err(ke)?;
          }
          MeshCombineOp::Join | MeshCombineOp::Cut => {
            let op = if *operation == MeshCombineOp::Join { BoolOp::Union } else { BoolOp::Subtract };
            for s in &shapes {
              shape = shape.boolean(s, op).map_err(ke)?;
            }
          }
        }
        self.state.bodies[ti] = self.state.bodies[ti].with_shape(shape, id);
        if !*keep_tools {
          self.state.bodies.retain(|b| !tools.contains(&b.id));
        }
        Ok(None)
      }
      FeatureKind::MeshSeparate { body, keep_body } => {
        let bi = self.body_index(*body)?;
        let (pos, tris) = self.mesh_of(bi)?;
        let parts = meshops::components(&pos, &tris);
        if parts.len() < 2 {
          return Err("the mesh body is a single shell: there is nothing to separate".into());
        }
        let mut shapes = Vec::new();
        for (p, t) in &parts {
          shapes.push(mesh_shape(p, t)?);
        }
        let (name, mesh) = (self.state.bodies[bi].name.clone(), self.state.bodies[bi].is_mesh());
        if !*keep_body {
          self.state.bodies.remove(bi);
        }
        // a mesh body's shells stay mesh bodies, named after it
        for (k, s) in shapes.into_iter().enumerate() {
          if mesh {
            self.new_mesh_body(id, s, format!("{name} ({})", k + 1));
          } else {
            self.new_body(id, s);
          }
        }
        Ok(None)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }
}
