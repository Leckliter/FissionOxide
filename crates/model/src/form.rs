//! Form (T-spline) bodies: a polygon control cage whose smooth limit surface is the body, as in
//! Fusion's Form workspace. The cage, its edits and the limit surface's B-spline patches live in
//! `fox-form`; this turns the patches into kernel surfaces.

pub use fox_form::{Cage as FormCage, Display, MAX_FACES, Selection, SymPlane, Symmetry, edit, prim};
use fox_kernel::BSplineSurface;

/// The limit surface as B-spline surfaces, one per B-rep face (see `fox_form::subd`).
pub fn surfaces(cage: &FormCage) -> Result<Vec<BSplineSurface>, String> {
  Ok(
    fox_form::subd::patches(cage)?
      .into_iter()
      .map(|p| BSplineSurface {
        degree: fox_form::Patch::DEGREE,
        poles: p.poles,
        u_knots: p.u_knots,
        u_mults: p.u_mults,
        v_knots: p.v_knots,
        v_mults: p.v_mults,
      })
      .collect(),
  )
}
