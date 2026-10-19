//! Solid modeling kernel for FissionOxide, built on OpenCASCADE (OCCT 7.8) through a small C++ bridge.
//!
//! The rest of the app talks to OCCT only through this crate's types, so the kernel can be
//! swapped. Every kernel call that can fail returns [`KernelError`]; OCCT exceptions are caught on
//! the C++ side and never abort the process.
//!
//! Units are millimetres and radians; world space is Z-up.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod ffi;

use std::path::Path;

use cxx::UniquePtr;
use fox_sketch::{Profile, Seg};
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

/// What a projected drawing line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
  Visible,
  /// A visible edge between tangent faces (drawn thin or left out).
  Tangent,
  /// A visible silhouette of a curved face.
  Outline,
  Hidden,
  HiddenOutline,
}

impl LineKind {
  pub fn hidden(self) -> bool {
    matches!(self, LineKind::Hidden | LineKind::HiddenOutline)
  }
}

/// A 2D polyline of a projection, in view coordinates (mm, x right, y up).
#[derive(Debug, Clone)]
pub struct ProjLine {
  pub kind: LineKind,
  pub points: Vec<[f64; 2]>,
}

/// A full circle seen face-on in a projection (holes, bosses).
#[derive(Debug, Clone, Copy)]
pub struct ProjCircle {
  pub center: [f64; 2],
  pub radius: f64,
  pub kind: LineKind,
}

#[derive(Debug, Clone, Default)]
pub struct Projection {
  pub lines: Vec<ProjLine>,
  pub circles: Vec<ProjCircle>,
}

impl Projection {
  /// Bounding box of the visible and hidden lines.
  pub fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
    let mut it = self.lines.iter().flat_map(|l| l.points.iter());
    let first = *it.next()?;
    Some(
      it.fold((first, first), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])),
    )
  }
}

fn line_kind(k: u8) -> LineKind {
  match k {
    0 => LineKind::Visible,
    1 => LineKind::Tangent,
    2 => LineKind::Outline,
    3 => LineKind::Hidden,
    _ => LineKind::HiddenOutline,
  }
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
    if r.solid_count() == 0 { Err(KernelError::Empty) } else { r.unify_internal() }
  }

  /// The part of `tool` (a face or shell) inside this solid: a section through it. Unlike
  /// [`Shape::boolean`] the result has no solids.
  pub fn section_inside(&self, tool: &Shape) -> Result<Shape> {
    Shape::wrap(sys::boolean_op(self.get()?, tool.get()?, 2, FUZZY)?)
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
  /// The outside surface is kept (walls grow inward). Walls meet in sharp corners, as in Fusion,
  /// or rounded ones when `round` is set.
  pub fn shell(&self, faces: &[u32], thickness: f64, round: bool) -> Result<Shape> {
    if !(thickness > 1e-9 && thickness.is_finite()) {
      return Err(KernelError::Input("shell thickness must be positive".into()));
    }
    let before = self.mass()?.volume;
    let r = Shape::wrap(sys::shell(self.get()?, faces, -thickness, round)?)?;
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

  /// For a sheet that was unfolded onto `face` (see [`Shape::unfold`]): the material to cut from
  /// this folded shape for the solids `removed` from its flat pattern.
  pub fn refold_cuts(&self, face: u32, thickness: f64, k_factor: f64, removed: &Shape) -> Result<Shape> {
    Shape::wrap(sys::refold_cuts(self.get()?, face, thickness, k_factor, removed.get()?)?)
  }

  /// Rounds edges (1-based) with a radius each.
  pub fn fillet_each(&self, edges: &[(u32, f64)]) -> Result<Shape> {
    if edges.iter().any(|(_, r)| !(*r > 1e-9 && r.is_finite())) {
      return Err(KernelError::Input("fillet radii must be positive".into()));
    }
    let (ids, radii): (Vec<u32>, Vec<f64>) = edges.iter().copied().unzip();
    Shape::wrap(sys::fillet_radii(self.get()?, &ids, &radii)?)
  }

  /// Hidden-line projection seen from `toward_viewer` with `right` as the screen's x axis.
  pub fn project(&self, toward_viewer: DVec3, right: DVec3, deflection: f64) -> Result<Projection> {
    let z = toward_viewer.try_normalize().ok_or(KernelError::Input("zero view direction".into()))?;
    let x = (right - z * right.dot(z)).try_normalize().ok_or(KernelError::Input("bad view right axis".into()))?;
    let d = sys::hlr_project(self.get()?, &[z.x, z.y, z.z, x.x, x.y, x.z], deflection.clamp(1e-4, 10.0))?;
    let pts: Vec<[f64; 2]> = d.points.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
    let mut lines = Vec::with_capacity(d.kinds.len());
    for (k, kind) in d.kinds.iter().enumerate() {
      let (Some(&a), Some(&b)) = (d.offsets.get(k), d.offsets.get(k + 1)) else { continue };
      if let Some(slice) = pts.get(a as usize..b as usize) {
        lines.push(ProjLine { kind: line_kind(*kind), points: slice.to_vec() });
      }
    }
    let circles = d
      .circles
      .chunks_exact(4)
      .map(|c| ProjCircle { center: [c[0], c[1]], radius: c[2], kind: line_kind(c[3] as u8) })
      .collect();
    Ok(Projection { lines, circles })
  }

  /// This planar face extruded `height` along its normal with sides drafted by `angle` radians
  /// (positive tapers inward).
  pub fn extrude_tapered(&self, height: f64, angle: f64) -> Result<Shape> {
    if !(height.is_finite() && height.abs() > 1e-9 && angle.is_finite() && angle.abs() < 1.5) {
      return Err(KernelError::Input("taper needs a height and an angle under 85°".into()));
    }
    // A straight prism with its sides drafted about the base plane keeps sharp corners, as in
    // Fusion; OCCT's drafted prism rounds them when the taper goes outward.
    self.drafted_prism(height, angle).or_else(|_| {
      // OCCT measures the height along the drafted sides
      Shape::wrap(sys::draft_prism(self.get()?, height / angle.cos(), angle)?)
    })
  }

  fn drafted_prism(&self, height: f64, angle: f64) -> Result<Shape> {
    let base = self.faces()?.into_iter().next().ok_or(KernelError::Input("taper needs a face".into()))?;
    let n = base.normal;
    let prism = self.extrude(n * height)?;
    let caps = |f: &FaceInfo| f.kind == SurfaceKind::Plane && f.normal.dot(n).abs() > 1.0 - 1e-9;
    let sides: Vec<u32> = prism.faces()?.iter().filter(|f| !caps(f)).map(|f| f.id).collect();
    let r = prism.draft(&sides, base.origin, n * height.signum(), angle)?;
    // the sides must have leaned the right way
    let (v, v0) = (r.mass()?.volume, prism.mass()?.volume);
    if r.solid_count() != 1 || (angle > 0.0) != (v < v0) {
      return Err(KernelError::Kernel("drafted prism went the wrong way".into()));
    }
    Ok(r)
  }

  /// This planar face grown (d > 0) or shrunk (d < 0) all round.
  pub fn offset_face(&self, d: f64) -> Result<Shape> {
    if !d.is_finite() {
      return Err(KernelError::Input("offset must be finite".into()));
    }
    Shape::wrap(sys::offset_face(self.get()?, d)?)
  }

  /// Chamfers with two distances (`angle` = false) or a distance and an angle in radians.
  pub fn chamfer_two(&self, edges: &[u32], d1: f64, d2: f64, angle: bool) -> Result<Shape> {
    if !(d1 > 1e-9 && d2 > 1e-9 && d1.is_finite() && d2.is_finite()) {
      return Err(KernelError::Input("chamfer sizes must be positive".into()));
    }
    Shape::wrap(sys::chamfer2(self.get()?, edges, d1, d2, angle)?)
  }

  /// Fillets edges with the radius running from `r1` to `r2` along each edge.
  pub fn fillet_variable(&self, edges: &[u32], r1: f64, r2: f64) -> Result<Shape> {
    if !(r1 > 1e-9 && r2 > 1e-9 && r1.is_finite() && r2.is_finite()) {
      return Err(KernelError::Input("fillet radii must be positive".into()));
    }
    Shape::wrap(sys::fillet_var(self.get()?, edges, r1, r2)?)
  }

  /// The pieces of this solid cut by another shape.
  pub fn split_by(&self, tool: &Shape) -> Result<Vec<Shape>> {
    Shape::wrap(sys::split_by_shape(self.get()?, tool.get()?)?)?.solids()
  }

  /// This shape split where `tool` crosses it, as one shape (surfaces included).
  pub fn split_raw(&self, tool: &Shape) -> Result<Shape> {
    Shape::wrap(sys::split_by_shape(self.get()?, tool.get()?)?)
  }

  /// One face (1-based face-map index) as a shape of its own.
  pub fn face_shape(&self, face: u32) -> Result<Shape> {
    Shape::wrap(sys::face_at_index(self.get()?, face)?)
  }

  /// One edge (1-based index, as in [`Shape::edges`]) as a shape of its own.
  pub fn edge_shape(&self, edge: u32) -> Result<Shape> {
    Shape::wrap(sys::edge_at_index(self.get()?, edge)?)
  }

  /// Minimum distance to another shape and the closest points on each.
  pub fn distance_to(&self, other: &Shape) -> Result<(f64, DVec3, DVec3)> {
    let v = sys::min_distance(self.get()?, other.get()?)?;
    match v.as_slice() {
      [d, a @ ..] if a.len() == 6 => Ok((*d, DVec3::new(a[0], a[1], a[2]), DVec3::new(a[3], a[4], a[5]))),
      _ => Err(KernelError::Kernel("measure returned no points".into())),
    }
  }

  /// Sweeps this profile (a face or wire in the plane through the axis, at `origin + radius·xdir`)
  /// along a helix of `radius` and `pitch` rising `height` along `dir`.
  #[allow(clippy::too_many_arguments)]
  pub fn helix_sweep(
    &self,
    origin: DVec3,
    dir: DVec3,
    xdir: DVec3,
    radius: f64,
    pitch: f64,
    height: f64,
    left: bool,
  ) -> Result<Shape> {
    let axis = [origin.x, origin.y, origin.z, dir.x, dir.y, dir.z, xdir.x, xdir.y, xdir.z];
    Shape::wrap(sys::helix_sweep(self.get()?, &axis, radius, pitch, height, left)?)
  }

  /// A face or shell thickened into a solid (positive = along the face normals).
  pub fn thicken(&self, d: f64) -> Result<Shape> {
    let s = Shape::wrap(sys::thicken(self.get()?, d)?)?;
    // offsetting against a face's normal can leave the solid inside out
    if s.mass().is_ok_and(|m| m.volume < 0.0) { s.reversed() } else { Ok(s) }
  }

  /// Scales about `center`, per axis (uniform factors keep the geometry exact).
  pub fn scaled(&self, center: DVec3, s: DVec3) -> Result<Shape> {
    Shape::wrap(sys::scale(self.get()?, center.x, center.y, center.z, s.x, s.y, s.z)?)
  }

  /// Removes faces and grows the neighbours to close the gap (fillets, holes, bosses...).
  pub fn remove_faces(&self, faces: &[u32]) -> Result<Shape> {
    Shape::wrap(sys::remove_faces(self.get()?, faces)?)
  }

  /// Splits faces where `tool` crosses them; the body stays in one piece.
  pub fn split_faces(&self, faces: &[u32], tool: &Shape) -> Result<Shape> {
    Shape::wrap(sys::split_faces(self.get()?, faces, tool.get()?)?)
  }

  /// A surface filling a closed boundary of edges (planar when the boundary is flat).
  pub fn patch(&self) -> Result<Shape> {
    Shape::wrap(sys::patch(self.get()?)?)
  }

  /// The first face's boundary loops as ordered polylines, outer loop first.
  pub fn face_loops(&self, deflection: f64) -> Result<Vec<Vec<DVec3>>> {
    let d = sys::face_loops(self.get()?, deflection.clamp(1e-4, 100.0))?;
    let pts: Vec<DVec3> = d.points.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
    Ok(d.offsets.windows(2).filter_map(|w| pts.get(w[0] as usize..w[1] as usize).map(<[DVec3]>::to_vec)).collect())
  }

  /// A single point (for measuring to a point).
  pub fn vertex(p: DVec3) -> Result<Shape> {
    Shape::wrap(sys::make_vertex(p.x, p.y, p.z)?)
  }

  /// Every loop of this planar face offset by `d` (negative = inwards) as ordered polylines;
  /// empty once the offset vanishes. An offset can split a region into several loops.
  pub fn offset_loops(&self, d: f64, deflection: f64) -> Result<Vec<Vec<DVec3>>> {
    let e = sys::offset_polylines(self.get()?, d, deflection.clamp(1e-4, 100.0))?;
    let pts: Vec<DVec3> = e.points.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
    Ok(
      e.offsets
        .windows(2)
        .filter_map(|w| pts.get(w[0] as usize..w[1] as usize).map(<[DVec3]>::to_vec))
        .filter(|l| l.len() > 2)
        .collect(),
    )
  }

  /// A ruled surface between this edge and another.
  pub fn ruled_to(&self, other: &Shape) -> Result<Shape> {
    Shape::wrap(sys::ruled(self.get()?, other.get()?)?)
  }

  /// Surfaces offset by `d` along their normals (a new surface, the original is kept).
  pub fn offset_surface(&self, d: f64) -> Result<Shape> {
    Shape::wrap(sys::offset_shape(self.get()?, d)?)
  }

  /// The same shape with its orientation (face normals) reversed.
  pub fn reversed(&self) -> Result<Shape> {
    Shape::wrap(sys::reversed(self.get()?)?)
  }

  pub fn solid_count(&self) -> usize {
    self.inner.as_ref().map(|s| sys::solid_count(s) as usize).unwrap_or(0)
  }

  /// The solids inside this shape (a cut can split one body into several).
  pub fn solids(&self) -> Result<Vec<Shape>> {
    let s = self.get()?;
    (0..sys::solid_count(s)).map(|i| Shape::wrap(sys::solid_at(s, i)?)).collect()
  }

  /// The bodies in this shape: its solids, or the shape itself when it has none (an open
  /// shell or loose surfaces from a mesh or IGES file).
  pub fn bodies(&self) -> Result<Vec<Shape>> {
    let solids = self.solids()?;
    if solids.is_empty() { Ok(vec![self.clone()]) } else { Ok(solids) }
  }

  /// Sews loose faces and shells together and closes watertight shells into solids.
  pub fn sewn(&self, tolerance: f64) -> Result<Shape> {
    Shape::wrap(sys::solidify(self.get()?, tolerance.clamp(1e-7, 1.0))?)
  }

  /// A faceted body from a triangle mesh (positions in mm, three indices per triangle). A
  /// watertight mesh becomes a solid, anything else an open shell. With `merge_coplanar`,
  /// coplanar triangles are merged into single faces (meshes exported from CAD get their flat
  /// faces back); this is skipped for very large meshes.
  pub fn from_mesh(positions: &[[f32; 3]], indices: &[u32], merge_coplanar: bool) -> Result<Shape> {
    if indices.len() < 3 || !indices.len().is_multiple_of(3) {
      return Err(KernelError::Input("the mesh has no triangles".into()));
    }
    if indices.iter().any(|&i| i as usize >= positions.len()) {
      return Err(KernelError::Input("mesh vertex index out of range".into()));
    }
    let pos: Vec<f64> = positions.iter().flat_map(|p| p.iter().map(|&c| c as f64)).collect();
    if pos.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite mesh coordinates".into()));
    }
    Shape::wrap(sys::mesh_to_shape(&pos, indices, merge_coplanar)?)
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

/// Reads the bodies of a STEP file: every solid, or, for a file of surfaces, the sewn shells.
pub fn read_step(path: &Path) -> Result<Vec<Shape>> {
  let p = path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))?;
  imported_bodies(Shape::wrap(sys::read_step(p)?)?)
}

/// Writes shapes to an IGES file (millimetres, B-rep entities).
pub fn write_iges<'a>(shapes: impl IntoIterator<Item = &'a Shape>, path: &Path) -> Result<()> {
  let c = Shape::compound(shapes)?;
  let p = path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))?;
  Ok(sys::write_iges(c.get()?, p)?)
}

/// Reads the bodies of an IGES file. IGES usually carries trimmed surfaces rather than solids,
/// so faces are sewn and closed shells become solids.
pub fn read_iges(path: &Path) -> Result<Vec<Shape>> {
  let p = path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))?;
  imported_bodies(Shape::wrap(sys::read_iges(p)?)?)
}

/// Writes shapes as an OCCT `.brep` file.
pub fn write_brep_file<'a>(shapes: impl IntoIterator<Item = &'a Shape>, path: &Path) -> Result<()> {
  let c = Shape::compound(shapes)?;
  std::fs::write(path, c.to_brep()?).map_err(|e| KernelError::Input(format!("{}: {e}", path.display())))
}

pub fn read_brep_file(path: &Path) -> Result<Vec<Shape>> {
  let text = std::fs::read_to_string(path).map_err(|e| KernelError::Input(format!("{}: {e}", path.display())))?;
  imported_bodies(Shape::from_brep(&text)?)
}

/// Solids if there are any; otherwise the faces sewn into shells (and solids where closed).
fn imported_bodies(s: Shape) -> Result<Vec<Shape>> {
  let s = if s.solid_count() == 0 { s.sewn(1e-3)? } else { s };
  let bodies = s.bodies()?;
  if bodies.is_empty() { Err(KernelError::Empty) } else { Ok(bodies) }
}

/// Writes meshes as one binary STL.
pub fn write_stl(meshes: &[TriMesh], path: &Path) -> std::io::Result<()> {
  let tris: usize = meshes.iter().map(|m| m.indices.len() / 3).sum();
  let mut out = Vec::with_capacity(84 + tris * 50);
  let mut header = [0u8; 80];
  let tag = b"FissionOxide binary STL";
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
  let mut s = String::from("# FissionOxide OBJ export (mm)\n");
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
  let mut put = |kind: f64, p: [DVec3; 4]| {
    out.push(kind);
    for v in p {
      out.extend_from_slice(&[v.x, v.y, v.z]);
    }
  };
  match *s {
    Seg::Line { a, b } => put(0.0, [plane.to_world(a), plane.to_world(b), DVec3::ZERO, DVec3::ZERO]),
    Seg::Arc { .. } => {
      put(1.0, [plane.to_world(s.start()), plane.to_world(s.mid()), plane.to_world(s.end()), DVec3::ZERO])
    }
    Seg::Circle { center, radius, ccw } => {
      let n = if ccw { plane.normal() } else { -plane.normal() };
      put(2.0, [plane.to_world(center), n, DVec3::new(radius, 0.0, 0.0), DVec3::ZERO])
    }
    Seg::Bezier { p } => put(3.0, p.map(|q| plane.to_world(q))),
  }
}

mod features2;
pub use features2::Inside;
mod surface;
pub use surface::{Extension, Untrim};
mod xcaf;
pub use xcaf::{StepBody, StepPart, read_step_assembly, write_step_assembly};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_features2;

mod sheet2;

mod refparts;
#[cfg(test)]
mod tests_refparts;
