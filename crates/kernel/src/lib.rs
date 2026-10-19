//! Solid modeling kernel for CLFusion, built on OpenCASCADE (OCCT 7.8) through a small C++ bridge.
//!
//! The rest of the app talks to OCCT only through this crate's types, so the kernel can be
//! swapped. Every kernel call that can fail returns [`KernelError`]; OCCT exceptions are caught on
//! the C++ side and never abort the process.
//!
//! Units are millimetres and radians; world space is Z-up.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod ffi;

use std::path::Path;

use clf_sketch::{Profile, Seg};
use cxx::UniquePtr;
use glam::{DMat4, DVec3};

use ffi::ffi as sys;

pub use glam;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum KernelError {
  #[error("{0}")]
  Kernel(String),
  #[error("the result is empty")]
  Empty,
  #[error("invalid input: {0}")]
  Input(String),
}

impl From<cxx::Exception> for KernelError {
  fn from(e: cxx::Exception) -> Self {
    KernelError::Kernel(e.what().to_string())
  }
}

pub type Result<T> = std::result::Result<T, KernelError>;

/// A plane with an in-plane coordinate system (sketch planes, construction planes, planar faces).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
  pub origin: DVec3,
  /// In-plane X axis (unit).
  pub x: DVec3,
  /// In-plane Y axis (unit, perpendicular to `x`).
  pub y: DVec3,
}

impl Plane {
  pub const XY: Plane = Plane { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Y };
  /// Front plane: X right, Z up (normal −Y, towards the viewer of the front view).
  pub const XZ: Plane = Plane { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Z };
  /// Right plane: Y right, Z up (normal +X).
  pub const YZ: Plane = Plane { origin: DVec3::ZERO, x: DVec3::Y, y: DVec3::Z };

  /// Builds a plane from an origin, a normal and a preferred X direction (re-orthogonalised).
  pub fn from_normal(origin: DVec3, normal: DVec3, xhint: DVec3) -> Result<Plane> {
    let n = normal.try_normalize().ok_or(KernelError::Input("zero normal".into()))?;
    let mut x = xhint - n * xhint.dot(n);
    if x.length_squared() < 1e-12 {
      // pick any perpendicular
      let a = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
      x = a - n * a.dot(n);
    }
    let x = x.normalize();
    let y = n.cross(x);
    Ok(Plane { origin, x, y })
  }

  pub fn normal(&self) -> DVec3 {
    self.x.cross(self.y)
  }

  pub fn to_world(&self, p: [f64; 2]) -> DVec3 {
    self.origin + self.x * p[0] + self.y * p[1]
  }

  pub fn to_local(&self, p: DVec3) -> [f64; 2] {
    let d = p - self.origin;
    [d.dot(self.x), d.dot(self.y)]
  }

  /// The same plane moved `d` along its normal.
  pub fn offset(&self, d: f64) -> Plane {
    Plane { origin: self.origin + self.normal() * d, ..*self }
  }

  fn data(&self) -> [f64; 9] {
    let n = self.normal();
    [self.origin.x, self.origin.y, self.origin.z, n.x, n.y, n.z, self.x.x, self.x.y, self.x.z]
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
  Union,
  Subtract,
  Intersect,
}

/// A kernel shape (a face, a solid, or a compound of solids).
pub struct Shape {
  inner: UniquePtr<sys::Shape>,
}

impl Clone for Shape {
  fn clone(&self) -> Self {
    match self.inner.as_ref() {
      Some(s) => Shape { inner: sys::shape_clone(s) },
      None => Shape { inner: UniquePtr::null() },
    }
  }
}

impl std::fmt::Debug for Shape {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Shape(type {})", self.inner.as_ref().map(sys::shape_type).unwrap_or(-1))
  }
}

/// Triangle mesh of a shape; `tri_face[i]` is the 1-based face index of triangle i.
#[derive(Debug, Clone, Default)]
pub struct TriMesh {
  pub positions: Vec<[f32; 3]>,
  pub normals: Vec<[f32; 3]>,
  pub indices: Vec<u32>,
  pub tri_face: Vec<u32>,
}

/// A displayable edge: its 1-based index in the shape's edge map and a polyline.
#[derive(Debug, Clone)]
pub struct EdgeLine {
  pub id: u32,
  pub points: Vec<DVec3>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
  Plane,
  Cylinder,
  Cone,
  Sphere,
  Torus,
  Other,
}

#[derive(Debug, Clone, Copy)]
pub struct FaceInfo {
  /// 1-based index in the shape's face map.
  pub id: u32,
  pub kind: SurfaceKind,
  /// Planes: centroid. Cylinders/spheres: axis location / centre.
  pub origin: DVec3,
  /// Planes: outward normal. Cylinders: axis direction.
  pub normal: DVec3,
  pub xdir: DVec3,
  pub center: DVec3,
  pub area: f64,
  pub radius: f64,
}

impl FaceInfo {
  /// The face's plane (with outward normal), if it is planar.
  pub fn plane(&self) -> Option<Plane> {
    (self.kind == SurfaceKind::Plane).then(|| Plane::from_normal(self.origin, self.normal, self.xdir).ok()).flatten()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
  Line,
  Circle,
  Other,
  Degenerate,
}

#[derive(Debug, Clone, Copy)]
pub struct EdgeInfo {
  pub id: u32,
  pub kind: CurveKind,
  pub start: DVec3,
  pub end: DVec3,
  pub mid: DVec3,
  pub center: DVec3,
  pub axis: DVec3,
  pub radius: f64,
  pub length: f64,
  /// Faces meeting at the edge (1-based face-map indices; 0 = none).
  pub faces: [u32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProps {
  /// mm³
  pub volume: f64,
  /// mm²
  pub area: f64,
  pub center_of_mass: DVec3,
  pub bbox_min: DVec3,
  pub bbox_max: DVec3,
}

/// A bend in a flat pattern: its centre line (in the flat), angle, and whether the bend's inside
/// is on the stationary face's side of the sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
  pub start: DVec3,
  pub end: DVec3,
  /// Radians.
  pub angle: f64,
  pub inner: bool,
}

/// Boolean fuzzy tolerance (mm): absorbs round-off between coincident faces.
const FUZZY: f64 = 1e-6;

impl Shape {
  fn wrap(p: UniquePtr<sys::Shape>) -> Result<Shape> {
    match p.as_ref() {
      Some(s) if !sys::shape_is_null(s) => Ok(Shape { inner: p }),
      _ => Err(KernelError::Empty),
    }
  }

  fn get(&self) -> Result<&sys::Shape> {
    self.inner.as_ref().ok_or(KernelError::Empty)
  }

  /// A planar face from a sketch profile on `plane`.
  pub fn face(plane: &Plane, profile: &Profile) -> Result<Shape> {
    let mut segs = Vec::new();
    let mut lens = Vec::new();
    for lp in std::iter::once(&profile.outer).chain(profile.holes.iter()) {
      lens.push(u32::try_from(lp.segs.len()).map_err(|_| KernelError::Input("too many segments".into()))?);
      for s in &lp.segs {
        push_seg(&mut segs, plane, s);
      }
    }
    if segs.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite profile coordinates".into()));
    }
    Shape::wrap(sys::make_face(&plane.data(), &segs, &lens)?)
  }

  /// Sweeps a face along a vector into a solid.
  pub fn extrude(&self, v: DVec3) -> Result<Shape> {
    if !(v.is_finite() && v.length() > 1e-9) {
      return Err(KernelError::Input("extrusion distance must not be zero".into()));
    }
    Shape::wrap(sys::make_prism(self.get()?, v.x, v.y, v.z)?)
  }

  /// Revolves a face around an axis by `angle` radians.
  pub fn revolve(&self, origin: DVec3, axis: DVec3, angle: f64) -> Result<Shape> {
    let a = axis.try_normalize().ok_or(KernelError::Input("zero axis".into()))?;
    if !(angle.is_finite() && angle.abs() > 1e-9) {
      return Err(KernelError::Input("revolve angle must not be zero".into()));
    }
    let angle = angle.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
    Shape::wrap(sys::make_revol(self.get()?, &[origin.x, origin.y, origin.z, a.x, a.y, a.z], angle)?)
  }

  pub fn make_box(a: DVec3, b: DVec3) -> Result<Shape> {
    let (lo, hi) = (a.min(b), a.max(b));
    let d = hi - lo;
    if !(d.is_finite() && d.min_element() > 1e-9) {
      return Err(KernelError::Input("box has no volume".into()));
    }
    Shape::wrap(sys::make_box(lo.x, lo.y, lo.z, hi.x, hi.y, hi.z)?)
  }

  pub fn cylinder(base: DVec3, axis: DVec3, radius: f64, height: f64) -> Result<Shape> {
    let a = axis.try_normalize().ok_or(KernelError::Input("zero axis".into()))?;
    if !(radius > 1e-9 && height.abs() > 1e-9 && radius.is_finite() && height.is_finite()) {
      return Err(KernelError::Input("cylinder needs a radius and height".into()));
    }
    let (a, base) = if height < 0.0 { (-a, base) } else { (a, base) };
    Shape::wrap(sys::make_cylinder(&[base.x, base.y, base.z, a.x, a.y, a.z], radius, height.abs())?)
  }

  pub fn sphere(center: DVec3, radius: f64) -> Result<Shape> {
    if !(radius > 1e-9 && radius.is_finite() && center.is_finite()) {
      return Err(KernelError::Input("sphere needs a radius".into()));
    }
    Shape::wrap(sys::make_sphere(center.x, center.y, center.z, radius)?)
  }

  pub fn torus(center: DVec3, axis: DVec3, major: f64, minor: f64) -> Result<Shape> {
    let a = axis.try_normalize().ok_or(KernelError::Input("zero axis".into()))?;
    if !(minor > 1e-9 && major > minor && major.is_finite()) {
      return Err(KernelError::Input("torus needs major radius > minor radius > 0".into()));
    }
    Shape::wrap(sys::make_torus(&[center.x, center.y, center.z, a.x, a.y, a.z], major, minor)?)
  }

  /// Boolean with another shape. A result with no solids (e.g. subtracting everything) is
  /// `Err(Empty)`.
  pub fn boolean(&self, tool: &Shape, op: BoolOp) -> Result<Shape> {
    let code = match op {
      BoolOp::Union => 0,
      BoolOp::Subtract => 1,
      BoolOp::Intersect => 2,
    };
    let r = Shape::wrap(sys::boolean_op(self.get()?, tool.get()?, code, FUZZY)?)?;
    if r.solid_count() == 0 { Err(KernelError::Empty) } else { Ok(r) }
  }

  /// Rounds the given edges (1-based edge-map indices).
  pub fn fillet(&self, edges: &[u32], radius: f64) -> Result<Shape> {
    if !(radius > 1e-9 && radius.is_finite()) {
      return Err(KernelError::Input("fillet radius must be positive".into()));
    }
    Shape::wrap(sys::fillet(self.get()?, edges, radius)?)
  }

  pub fn chamfer(&self, edges: &[u32], distance: f64) -> Result<Shape> {
    if !(distance > 1e-9 && distance.is_finite()) {
      return Err(KernelError::Input("chamfer distance must be positive".into()));
    }
    Shape::wrap(sys::chamfer(self.get()?, edges, distance)?)
  }

  /// Hollows the solid to `thickness`, opening the given faces (1-based face-map indices).
  /// The outside surface is kept (walls grow inward).
  pub fn shell(&self, faces: &[u32], thickness: f64) -> Result<Shape> {
    if !(thickness > 1e-9 && thickness.is_finite()) {
      return Err(KernelError::Input("shell thickness must be positive".into()));
    }
    let before = self.mass()?.volume;
    let r = Shape::wrap(sys::shell(self.get()?, faces, -thickness)?)?;
    // OCCT can "succeed" with nonsense when the walls swallow the part.
    let after = r.mass()?.volume;
    if !(after > 1e-9 && after < before) || r.solid_count() == 0 {
      return Err(KernelError::Kernel("shell failed (the walls are too thick for this body)".into()));
    }
    Ok(r)
  }

  /// Applies an affine transform.
  pub fn transformed(&self, m: DMat4) -> Result<Shape> {
    let c = m.to_cols_array_2d();
    // gp_Trsf wants rows of the 3x4 part
    let rows =
      [c[0][0], c[1][0], c[2][0], c[3][0], c[0][1], c[1][1], c[2][1], c[3][1], c[0][2], c[1][2], c[2][2], c[3][2]];
    if rows.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite transform".into()));
    }
    Shape::wrap(sys::transform(self.get()?, &rows)?)
  }

  pub fn translated(&self, v: DVec3) -> Result<Shape> {
    self.transformed(DMat4::from_translation(v))
  }

  /// Rotates about an axis through `origin`.
  pub fn rotated(&self, origin: DVec3, axis: DVec3, angle: f64) -> Result<Shape> {
    let a = axis.try_normalize().ok_or(KernelError::Input("zero axis".into()))?;
    let m = DMat4::from_translation(origin) * DMat4::from_axis_angle(a, angle) * DMat4::from_translation(-origin);
    self.transformed(m)
  }

  pub fn mirrored(&self, origin: DVec3, normal: DVec3) -> Result<Shape> {
    let n = normal.try_normalize().ok_or(KernelError::Input("zero normal".into()))?;
    Shape::wrap(sys::mirror(self.get()?, &[origin.x, origin.y, origin.z, n.x, n.y, n.z])?)
  }

  /// Merges coplanar faces / collinear edges left over from modeling operations.
  pub fn unified(&self) -> Result<Shape> {
    Shape::wrap(sys::unify(self.get()?)?)
  }

  /// Flattens a sheet metal solid into the plane of `face` (1-based, planar), which stays put.
  /// Bends are rolled out to the length of their neutral layer, `inner radius + k_factor ×
  /// thickness` from the axis.
  pub fn unfold(&self, face: u32, thickness: f64, k_factor: f64) -> Result<(Shape, Vec<Bend>)> {
    if !(thickness > 1e-9 && thickness.is_finite()) {
      return Err(KernelError::Input("sheet thickness must be positive".into()));
    }
    if !(0.0..=1.0).contains(&k_factor) {
      return Err(KernelError::Input("the K-factor must be between 0 and 1".into()));
    }
    let mut raw = Vec::new();
    let flat = Shape::wrap(sys::unfold(self.get()?, face, thickness, k_factor, &mut raw)?)?;
    let bends = raw
      .chunks_exact(8)
      .map(|c| Bend {
        start: DVec3::new(c[0], c[1], c[2]),
        end: DVec3::new(c[3], c[4], c[5]),
        angle: c[6],
        inner: c[7] > 0.5,
      })
      .collect();
    Ok((flat, bends))
  }

  /// An open chain of sketch segments on `plane` as a wire (sweep paths).
  pub fn path(plane: &Plane, segs: &[Seg]) -> Result<Shape> {
    let mut data = Vec::new();
    for s in segs {
      push_seg(&mut data, plane, s);
    }
    if data.is_empty() || data.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("the path is empty or not finite".into()));
    }
    Shape::wrap(sys::make_wire(&data)?)
  }

  /// Sweeps this face along a path wire.
  pub fn sweep(&self, path: &Shape) -> Result<Shape> {
    let r = Shape::wrap(sys::make_pipe(self.get()?, path.get()?)?)?;
    if r.solid_count() == 0 { Err(KernelError::Kernel("the sweep made no solid".into())) } else { Ok(r) }
  }

  /// A solid through the outer boundaries of the given faces, in order.
  pub fn loft(sections: &[Shape], ruled: bool, closed: bool) -> Result<Shape> {
    if sections.len() < 2 {
      return Err(KernelError::Input("a loft needs at least two profiles".into()));
    }
    let c = Shape::compound(sections)?;
    let r = Shape::wrap(sys::make_loft(c.get()?, ruled, closed)?)?;
    if r.solid_count() == 0 { Err(KernelError::Kernel("the loft made no solid".into())) } else { Ok(r) }
  }

  /// The solids on either side of a plane (one or more pieces).
  pub fn split(&self, origin: DVec3, normal: DVec3) -> Result<Vec<Shape>> {
    let n = normal.try_normalize().ok_or(KernelError::Input("zero normal".into()))?;
    let m = self.mass()?;
    let size =
      4.0 * ((m.bbox_max - m.bbox_min).length() + (m.bbox_max + m.bbox_min).length() / 2.0 + origin.length()) + 10.0;
    let r = Shape::wrap(sys::split_by_plane(self.get()?, &[origin.x, origin.y, origin.z, n.x, n.y, n.z], size)?)?;
    r.solids()
  }

  /// Tapers faces (1-based) by `angle` radians about a neutral plane, pulling along its normal.
  pub fn draft(&self, faces: &[u32], origin: DVec3, pull: DVec3, angle: f64) -> Result<Shape> {
    let n = pull.try_normalize().ok_or(KernelError::Input("zero pull direction".into()))?;
    if !(angle.is_finite() && angle.abs() < std::f64::consts::FRAC_PI_2) {
      return Err(KernelError::Input("the draft angle must be under 90°".into()));
    }
    Shape::wrap(sys::draft(self.get()?, faces, &[origin.x, origin.y, origin.z, n.x, n.y, n.z], angle)?)
  }

  /// One face (1-based face-map index) as a shape of its own.
  pub fn face_shape(&self, face: u32) -> Result<Shape> {
    Shape::wrap(sys::face_at_index(self.get()?, face)?)
  }

  pub fn solid_count(&self) -> usize {
    self.inner.as_ref().map(|s| sys::solid_count(s) as usize).unwrap_or(0)
  }

  /// The solids inside this shape (a cut can split one body into several).
  pub fn solids(&self) -> Result<Vec<Shape>> {
    let s = self.get()?;
    (0..sys::solid_count(s)).map(|i| Shape::wrap(sys::solid_at(s, i)?)).collect()
  }

  pub fn compound<'a>(shapes: impl IntoIterator<Item = &'a Shape>) -> Result<Shape> {
    let mut c = sys::compound_new();
    for s in shapes {
      if let (Some(cp), Some(sp)) = (c.as_mut(), s.inner.as_ref()) {
        sys::compound_add(cp, sp)?;
      }
    }
    Shape::wrap(c)
  }

  /// Triangulates for display / STL. `tolerance` is the chordal deviation in mm.
  pub fn tessellate(&self, tolerance: f64) -> Result<TriMesh> {
    let tol = if tolerance.is_finite() { tolerance.clamp(1e-4, 100.0) } else { 0.1 };
    let m = sys::mesh_shape(self.get()?, tol, 0.35)?;
    let positions = m.positions.chunks_exact(3).map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]).collect();
    let normals = m.normals.chunks_exact(3).map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]).collect();
    Ok(TriMesh { positions, normals, indices: m.triangles, tri_face: m.tri_face })
  }

  /// Edge polylines for display and picking.
  pub fn edge_lines(&self, deflection: f64) -> Result<Vec<EdgeLine>> {
    let d = sys::edge_polylines(self.get()?, deflection.clamp(1e-4, 100.0))?;
    let pts: Vec<DVec3> = d.points.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
    let mut out = Vec::with_capacity(d.edge_ids.len());
    for (k, id) in d.edge_ids.iter().enumerate() {
      let (Some(&a), Some(&b)) = (d.offsets.get(k), d.offsets.get(k + 1)) else { continue };
      if let Some(slice) = pts.get(a as usize..b as usize) {
        out.push(EdgeLine { id: *id, points: slice.to_vec() });
      }
    }
    Ok(out)
  }

  pub fn faces(&self) -> Result<Vec<FaceInfo>> {
    let v = sys::face_infos(self.get()?)?;
    Ok(
      v.iter()
        .enumerate()
        .map(|(i, f)| FaceInfo {
          id: i as u32 + 1,
          kind: match f.kind {
            0 => SurfaceKind::Plane,
            1 => SurfaceKind::Cylinder,
            2 => SurfaceKind::Cone,
            3 => SurfaceKind::Sphere,
            4 => SurfaceKind::Torus,
            _ => SurfaceKind::Other,
          },
          origin: DVec3::from(f.origin),
          normal: DVec3::from(f.normal),
          xdir: DVec3::from(f.xdir),
          center: DVec3::from(f.center),
          area: f.area,
          radius: f.radius,
        })
        .collect(),
    )
  }

  pub fn edges(&self) -> Result<Vec<EdgeInfo>> {
    let v = sys::edge_infos(self.get()?)?;
    Ok(
      v.iter()
        .enumerate()
        .map(|(i, e)| EdgeInfo {
          id: i as u32 + 1,
          kind: match e.kind {
            0 => CurveKind::Line,
            1 => CurveKind::Circle,
            2 => CurveKind::Other,
            _ => CurveKind::Degenerate,
          },
          start: DVec3::from(e.p0),
          end: DVec3::from(e.p1),
          mid: DVec3::from(e.mid),
          center: DVec3::from(e.center),
          axis: DVec3::from(e.axis),
          radius: e.radius,
          length: e.length,
          faces: [e.face1, e.face2],
        })
        .collect(),
    )
  }

  pub fn mass(&self) -> Result<MassProps> {
    let m = sys::mass_props(self.get()?)?;
    Ok(MassProps {
      volume: m.volume,
      area: m.area,
      center_of_mass: DVec3::from(m.cog),
      bbox_min: DVec3::from(m.bbox_min),
      bbox_max: DVec3::from(m.bbox_max),
    })
  }

  pub fn is_valid(&self) -> bool {
    self.inner.as_ref().is_some_and(sys::is_valid)
  }

  /// OCCT's text BRep format (lossless; used to store imported bodies in a design file).
  pub fn to_brep(&self) -> Result<String> {
    Ok(sys::write_brep(self.get()?)?)
  }

  pub fn from_brep(data: &str) -> Result<Shape> {
    Shape::wrap(sys::read_brep(data)?)
  }
}

/// Writes shapes to a STEP (AP214) file.
pub fn write_step<'a>(shapes: impl IntoIterator<Item = &'a Shape>, path: &Path) -> Result<()> {
  let c = Shape::compound(shapes)?;
  let p = path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))?;
  Ok(sys::write_step(c.get()?, p)?)
}

/// Reads every solid from a STEP file.
pub fn read_step(path: &Path) -> Result<Vec<Shape>> {
  let p = path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))?;
  let s = Shape::wrap(sys::read_step(p)?)?;
  let solids = s.solids()?;
  if solids.is_empty() { Err(KernelError::Empty) } else { Ok(solids) }
}

/// Writes meshes as one binary STL.
pub fn write_stl(meshes: &[TriMesh], path: &Path) -> std::io::Result<()> {
  let tris: usize = meshes.iter().map(|m| m.indices.len() / 3).sum();
  let mut out = Vec::with_capacity(84 + tris * 50);
  let mut header = [0u8; 80];
  let tag = b"CLFusion binary STL";
  header[..tag.len()].copy_from_slice(tag);
  out.extend_from_slice(&header);
  out.extend_from_slice(&(u32::try_from(tris).unwrap_or(u32::MAX)).to_le_bytes());
  for m in meshes {
    for t in m.indices.chunks_exact(3) {
      let p: Vec<[f32; 3]> = t.iter().map(|&i| m.positions.get(i as usize).copied().unwrap_or([0.0; 3])).collect();
      let (a, b, c) = (glam::Vec3::from(p[0]), glam::Vec3::from(p[1]), glam::Vec3::from(p[2]));
      let n = (b - a).cross(c - a).normalize_or_zero();
      for v in [n, a, b, c] {
        for k in v.to_array() {
          out.extend_from_slice(&k.to_le_bytes());
        }
      }
      out.extend_from_slice(&[0, 0]);
    }
  }
  std::fs::write(path, out)
}

/// Writes meshes as a Wavefront OBJ (one object per mesh).
pub fn write_obj(meshes: &[(String, TriMesh)], path: &Path) -> std::io::Result<()> {
  use std::fmt::Write as _;
  let mut s = String::from("# CLFusion OBJ export (mm)\n");
  let mut base = 1usize;
  for (name, m) in meshes {
    let _ = writeln!(s, "o {}", name.replace(char::is_whitespace, "_"));
    for p in &m.positions {
      let _ = writeln!(s, "v {} {} {}", p[0], p[1], p[2]);
    }
    for n in &m.normals {
      let _ = writeln!(s, "vn {} {} {}", n[0], n[1], n[2]);
    }
    for t in m.indices.chunks_exact(3) {
      let (a, b, c) = (t[0] as usize + base, t[1] as usize + base, t[2] as usize + base);
      let _ = writeln!(s, "f {a}//{a} {b}//{b} {c}//{c}");
    }
    base += m.positions.len();
  }
  std::fs::write(path, s)
}

fn push_seg(out: &mut Vec<f64>, plane: &Plane, s: &Seg) {
  let mut put = |kind: f64, p: [DVec3; 3]| {
    out.push(kind);
    for v in p {
      out.extend_from_slice(&[v.x, v.y, v.z]);
    }
  };
  match *s {
    Seg::Line { a, b } => put(0.0, [plane.to_world(a), plane.to_world(b), DVec3::ZERO]),
    Seg::Arc { .. } => put(1.0, [plane.to_world(s.start()), plane.to_world(s.mid()), plane.to_world(s.end())]),
    Seg::Circle { center, radius, ccw } => {
      let n = if ccw { plane.normal() } else { -plane.normal() };
      put(2.0, [plane.to_world(center), n, DVec3::new(radius, 0.0, 0.0)])
    }
  }
}

#[cfg(test)]
mod tests;
