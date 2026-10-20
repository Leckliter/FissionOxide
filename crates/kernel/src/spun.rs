//! A body's reach from an axis along it: the raw material of the sketch's Spun Profile (the region
//! a body sweeps through in a half-plane bounded by the axis as it turns about the axis). The C++
//! side is `cpp/spun.cpp`; fox-model fits sketch curves to the spans.

use glam::DVec3;

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

/// Where a body starts and ends along an axis, and the stations in between where its outline may
/// turn a corner (vertices, and points where an edge turns back along the axis).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AxialEvents {
  pub tmin: f64,
  pub tmax: f64,
  pub stations: Vec<f64>,
  /// Each vertex as (station, distance from the axis).
  pub vertices: Vec<(f64, f64)>,
}

fn axis_data(origin: DVec3, dir: DVec3) -> Result<[f64; 6]> {
  let d = dir.try_normalize().ok_or(KernelError::Input("the axis has no direction".into()))?;
  if !origin.is_finite() {
    return Err(KernelError::Input("the axis is not finite".into()));
  }
  Ok([origin.x, origin.y, origin.z, d.x, d.y, d.z])
}

impl Shape {
  /// This body's extent along the axis through `origin` along `dir` (stations measured from
  /// `origin`), with its corner stations and vertices.
  pub fn axial_events(&self, origin: DVec3, dir: DVec3) -> Result<AxialEvents> {
    let v = sys::axial_events(self.get()?, &axis_data(origin, dir)?)?;
    let bad = || KernelError::Kernel("malformed axial events".into());
    let n = *v.get(2).ok_or_else(bad)? as usize;
    let stations = v.get(3..3 + n).ok_or_else(bad)?.to_vec();
    let vertices = v[3 + n..].chunks_exact(2).map(|c| (c[0], c[1])).collect();
    Ok(AxialEvents { tmin: v[0], tmax: v[1], stations, vertices })
  }

  /// The body's cross-sections across the axis at each station in `ts`: per station, each
  /// section face's (nearest, farthest) distance from the axis. Empty where the plane misses.
  pub fn radial_slices(&self, origin: DVec3, dir: DVec3, ts: &[f64]) -> Result<Vec<Vec<(f64, f64)>>> {
    if ts.iter().any(|t| !t.is_finite()) {
      return Err(KernelError::Input("a station is not finite".into()));
    }
    let v = sys::radial_slices(self.get()?, &axis_data(origin, dir)?, ts)?;
    let mut out = Vec::with_capacity(ts.len());
    let mut i = 0;
    for _ in ts {
      let n = *v.get(i).ok_or(KernelError::Kernel("malformed radial slices".into()))? as usize;
      let spans = v.get(i + 1..i + 1 + 2 * n).ok_or(KernelError::Kernel("malformed radial slices".into()))?;
      out.push(spans.chunks_exact(2).map(|c| (c[0], c[1])).collect());
      i += 1 + 2 * n;
    }
    Ok(out)
  }
}
