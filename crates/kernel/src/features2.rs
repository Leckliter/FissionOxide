//! The second batch of solid features: guided and twisted sweeps, lofts along a centre line or
//! rails, emboss, boundary fill cells, extended surfaces (replace face, extrude to object) and
//! setback fillet corners. The C++ side is `cpp/features2.cpp`.

use glam::DVec3;

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

/// How a sweep steers and shapes its profile along the path.
#[derive(Debug, Clone, Copy, Default)]
pub struct SweepOptions<'a> {
  /// A guide rail: the section turns to face it.
  pub rail: Option<&'a Shape>,
  /// With a rail: the section scales about the path as the rail's distance from it changes.
  pub scale: bool,
  /// Turn about the path from start to end, radians (no rail).
  pub twist: f64,
  /// The section grows all round by tan(taper) per unit of path length, radians.
  pub taper: f64,
  /// The section keeps its orientation and only moves along the path.
  pub parallel: bool,
}

/// Where a point lies relative to a solid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inside {
  In,
  Out,
  On,
}

impl Shape {
  fn solid_or(self, what: &str) -> Result<Shape> {
    if self.solid_count() == 0 { Err(KernelError::Kernel(format!("the {what} made no solid"))) } else { Ok(self) }
  }

  /// Sweeps this profile along a path wire, oriented and scaled to keep touching a guide rail.
  pub fn sweep_rail(&self, path: &Shape, rail: &Shape) -> Result<Shape> {
    self.sweep_guided(path, &SweepOptions { rail: Some(rail), scale: true, ..Default::default() })
  }

  /// Sweeps this profile along a path wire, turning it by `twist` radians from start to end.
  pub fn sweep_twisted(&self, path: &Shape, twist: f64) -> Result<Shape> {
    self.sweep_guided(path, &SweepOptions { twist, ..Default::default() })
  }

  /// Sweeps this profile along a path wire, steered and shaped by `opts`.
  pub fn sweep_guided(&self, path: &Shape, opts: &SweepOptions) -> Result<Shape> {
    if !opts.twist.is_finite() || !opts.taper.is_finite() {
      return Err(KernelError::Input("the twist or taper angle is not finite".into()));
    }
    if opts.taper.abs() >= std::f64::consts::FRAC_PI_2 {
      return Err(KernelError::Input("the taper angle must be under 90 degrees".into()));
    }
    let rail = opts.rail.unwrap_or(path);
    let r = sys::sweep_guided(
      self.get()?,
      path.get()?,
      rail.get()?,
      opts.rail.is_some(),
      opts.scale,
      opts.twist,
      opts.taper,
      opts.parallel,
    )?;
    Shape::wrap(r)?.solid_or("sweep")
  }

  /// A loft through the outer boundaries of `sections`, in order, along a centre line wire.
  pub fn loft_centerline(sections: &[Shape], centerline: &Shape) -> Result<Shape> {
    if sections.len() < 2 {
      return Err(KernelError::Input("a loft needs at least two profiles".into()));
    }
    let c = Shape::compound(sections)?;
    Shape::wrap(sys::sweep_sections(c.get()?, centerline.get()?)?)?.solid_or("loft")
  }

  /// A loft through the outer boundaries of `sections` whose sides follow rail wires.
  pub fn loft_rails(sections: &[Shape], rails: &[Shape]) -> Result<Shape> {
    if sections.len() < 2 {
      return Err(KernelError::Input("a loft needs at least two profiles".into()));
    }
    if rails.is_empty() {
      return Err(KernelError::Input("no rails".into()));
    }
    let (c, r) = (Shape::compound(sections)?, Shape::compound(rails)?);
    Shape::wrap(sys::loft_rails(c.get()?, r.get()?)?)?.solid_or("loft")
  }

  /// The material an emboss adds to (or a deboss removes from) this body: `profiles` (planar
  /// faces) projected along `dir` onto face `face` (1-based), `depth` deep. With `wrap` they are
  /// wrapped around a cylindrical face instead, keeping their lengths.
  pub fn emboss_tool(
    &self,
    face: u32,
    profiles: &[Shape],
    dir: DVec3,
    depth: f64,
    deboss: bool,
    wrap: bool,
  ) -> Result<Shape> {
    if !(depth.is_finite() && depth > 1e-9) {
      return Err(KernelError::Input("the depth must be positive".into()));
    }
    let p = Shape::compound(profiles)?;
    Shape::wrap(sys::emboss_tool(self.get()?, face, p.get()?, &dir.to_array(), depth, deboss, wrap)?)
  }

  /// The closed cells bounded by these bodies and surfaces (boundary fill), as separate solids.
  pub fn boundary_cells(tools: &[Shape]) -> Result<Vec<Shape>> {
    let c = Shape::compound(tools)?;
    Shape::wrap(sys::boundary_cells(c.get()?)?)?.solids()
  }

  /// Is the point inside this solid?
  pub fn classify(&self, p: DVec3) -> Result<Inside> {
    Ok(match sys::classify(self.get()?, p.x, p.y, p.z)? {
      0 => Inside::In,
      2 => Inside::On,
      _ => Inside::Out,
    })
  }

  /// A square planar face `2·size` wide, centred at `origin`.
  pub fn plane_face(origin: DVec3, normal: DVec3, size: f64) -> Result<Shape> {
    let n = normal.try_normalize().ok_or(KernelError::Input("zero normal".into()))?;
    Shape::wrap(sys::plane_face(&[origin.x, origin.y, origin.z, n.x, n.y, n.z], size)?)
  }

  /// This face's surface grown by `size` past its edges (closed directions stay whole), moved
  /// `offset` towards `toward` where the surface is nearest `at`. It keeps the face's orientation.
  pub fn extended_face(&self, size: f64, offset: f64, at: DVec3, toward: DVec3) -> Result<Shape> {
    Shape::wrap(sys::extended_face(self.get()?, size, offset, &at.to_array(), &toward.to_array())?)
  }

  /// Signed distance of a point from this face's surface, positive on the side its normal faces.
  pub fn side_of(&self, p: DVec3) -> Result<f64> {
    Ok(sys::side_of(self.get()?, p.x, p.y, p.z)?)
  }

  /// Re-blends filleted corners: everything within `radius` of each point is replaced by a
  /// smooth patch tangent to the faces around it (setback corners).
  pub fn setback_corners(&self, corners: &[DVec3], radius: f64) -> Result<Shape> {
    if !(radius.is_finite() && radius > 1e-9) {
      return Err(KernelError::Input("the setback must be positive".into()));
    }
    let pts: Vec<f64> = corners.iter().flat_map(|p| p.to_array()).collect();
    Shape::wrap(sys::setback_corners(self.get()?, &pts, radius)?)
  }
}
