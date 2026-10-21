//! Finite element simulation: Fusion's Simulation workspace studies on quadratic tetrahedra.
//!
//! - [`surface`]: the closed triangle surface of a body (welded from a tessellation, every
//!   triangle tagged with its CAD face), remeshed to the element size.
//! - [`mesh`]: tetrahedral meshing ([`mesh::mesh_solid`]): an octree/BCC lattice of interior
//!   points, a conforming Delaunay tetrahedralization ([`delaunay`]) and 10-node elements, with
//!   local refinement on faces.
//! - [`fem`]: exact element matrices (stiffness, mass, conduction, loads) of straight-sided
//!   10-node tetrahedra and 6-node boundary triangles.
//! - [`linalg`]: the sparse symmetric matrix, sparse Cholesky ([`faer`]), preconditioned
//!   conjugate gradients and shift-invert Lanczos.
//! - [`study`]: a [`Model`] (meshed parts with materials, constraints, loads, bonded contacts)
//!   and the solves: [`solve_static`] (displacement, stress, safety factor, reactions),
//!   [`solve_modal`] (natural frequencies and mode shapes) and [`solve_thermal`] (steady-state
//!   temperature and heat flux). [`StaticProblem`] can be solved repeatedly with per-element
//!   stiffness scales, which is what topology optimization needs.
//! - [`results`]: fields by name, probes, the contour colour map, the drawn surface and reports.
//!
//! Units throughout: mm, N, MPa, tonne, s, W, °C.

// NaN-rejecting checks (`!(x > 0.0)`) and index loops over element matrices read best as they are
#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop)]

pub mod delaunay;
pub mod fem;
pub mod geom;
pub mod linalg;
pub mod material;
pub mod mesh;
pub mod results;
pub mod study;
pub mod surface;
#[cfg(test)]
mod tests;

pub use material::Material;
pub use mesh::{MeshOptions, MeshStats, Refinement, TetMesh, mesh_solid};
pub use results::{Solution, colormap, probe, report};
pub use study::{
  Constraint, Contacts, FaceSel, Load, Model, Part, SolveOptions, StaticProblem, StaticSolution, ThermalLoad,
  solve_modal, solve_static, solve_thermal,
};
pub use surface::SurfaceMesh;

/// The three studies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyKind {
  /// Linear static stress.
  #[default]
  Static,
  /// Modal frequencies.
  Modal,
  /// Steady-state heat transfer.
  Thermal,
}

impl StudyKind {
  pub fn label(self) -> &'static str {
    match self {
      StudyKind::Static => "Static Stress",
      StudyKind::Modal => "Modal Frequencies",
      StudyKind::Thermal => "Thermal",
    }
  }
}

/// Runs a study.
pub fn solve(kind: StudyKind, model: &Model, options: SolveOptions) -> Result<Solution, String> {
  Ok(match kind {
    StudyKind::Static => Solution::Static(solve_static(model, options)?),
    StudyKind::Modal => Solution::Modal(solve_modal(model, options)?),
    StudyKind::Thermal => Solution::Thermal(solve_thermal(model, options)?),
  })
}
