//! Assemblies: components (named groups of bodies) and the joints that position them.
//!
//! A joint mates a joint origin on the moving component to one on the fixed component (or the
//! world): the origins' points coincide and their Z axes face each other (so faces mate), then
//! the joint's own motion — rotation about Z, slides along Z or X/Y, tilt — moves the component
//! within the degrees of freedom of the joint type. Motion values are expressions, so parameters
//! drive mechanisms and limits are checked on every rebuild.

use fox_kernel::glam::{DMat4, DQuat, DVec3};
use fox_kernel::{CurveKind, EdgeInfo, FaceInfo, SurfaceKind};
use serde::{Deserialize, Serialize};

use crate::refs::{EdgeRef, FaceRef};
use crate::{BodyId, FeatureId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ComponentId(pub u32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
  pub id: ComponentId,
  pub name: String,
  pub bodies: Vec<BodyId>,
  /// Grounded components never move.
  #[serde(default)]
  pub grounded: bool,
  /// Physical material (see [`MATERIALS`]); empty = steel.
  #[serde(default)]
  pub material: String,
  /// The component this one belongs to (sub-assemblies); `None` = the top level. Joints that
  /// move a component move everything below it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub parent: Option<ComponentId>,
}

/// Physical materials: name and density in g/cm³.
pub const MATERIALS: &[(&str, f64)] = &[
  ("Steel", 7.85),
  ("Stainless Steel", 8.0),
  ("Aluminum", 2.70),
  ("Brass", 8.50),
  ("Copper", 8.96),
  ("Titanium", 4.43),
  ("Cast Iron", 7.20),
  ("ABS Plastic", 1.04),
  ("PLA", 1.24),
  ("Nylon", 1.15),
  ("Polycarbonate", 1.20),
  ("Acrylic", 1.18),
  ("Wood", 0.70),
  ("Rubber", 1.10),
];

/// Density (g/cm³) of a material name (case-insensitive); unknown/empty = steel.
pub fn density(material: &str) -> f64 {
  MATERIALS.iter().find(|(n, _)| n.eq_ignore_ascii_case(material.trim())).map(|(_, d)| *d).unwrap_or(7.85)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
  #[default]
  Rigid,
  /// Rotates about Z.
  Revolute,
  /// Slides along Z.
  Slider,
  /// Rotates about and slides along Z.
  Cylindrical,
  /// Rotates about Z, slides along X.
  PinSlot,
  /// Slides in the XY plane, rotates about Z.
  Planar,
  /// Rotates about Z and tilts about X.
  Ball,
  /// Rotates about Z, and slides along Z by the joint's lead per turn (a leadscrew or bolt).
  Screw,
}

impl JointKind {
  /// Which motions the joint allows: (rotation, slide, slide2, pitch).
  pub fn freedoms(self) -> (bool, bool, bool, bool) {
    match self {
      JointKind::Rigid => (false, false, false, false),
      JointKind::Revolute => (true, false, false, false),
      JointKind::Slider => (false, true, false, false),
      JointKind::Cylindrical | JointKind::PinSlot => (true, true, false, false),
      JointKind::Planar => (true, true, true, false),
      JointKind::Ball => (true, false, false, true),
      // the slide follows the rotation (see `screw_slide`)
      JointKind::Screw => (true, true, false, false),
    }
  }
}

/// Optional motion limits, as (min, max) expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct JointLimits {
  #[serde(default)]
  pub rotation: Option<(String, String)>,
  #[serde(default)]
  pub slide: Option<(String, String)>,
}

/// Where a joint origin sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JointGeometry {
  /// The centre of a face; Z = its outward normal (planes) or axis (cylinders).
  Face { face: FaceRef },
  /// The centre of a circular edge; Z points out of the flat face it bounds.
  Edge { edge: EdgeRef },
  /// An explicit point and axes (world).
  Point {
    point: [f64; 3],
    #[serde(default = "z_axis")]
    z: [f64; 3],
    #[serde(default = "x_axis")]
    x: [f64; 3],
  },
  /// A saved joint origin (a `JointOrigin` feature): it moves with its component.
  Origin { origin: FeatureId },
}

fn z_axis() -> [f64; 3] {
  [0.0, 0.0, 1.0]
}

fn x_axis() -> [f64; 3] {
  [1.0, 0.0, 0.0]
}

impl JointGeometry {
  /// The body the origin lies on, if any.
  pub fn body(&self) -> Option<BodyId> {
    match self {
      JointGeometry::Face { face } => Some(face.body),
      JointGeometry::Edge { edge } => Some(edge.body),
      JointGeometry::Point { .. } | JointGeometry::Origin { .. } => None,
    }
  }

  /// The saved joint origin this uses, if any.
  pub fn origin(&self) -> Option<FeatureId> {
    if let JointGeometry::Origin { origin } = self { Some(*origin) } else { None }
  }
}

/// A joint origin's frame: X, Y, Z axes and origin as a matrix.
pub fn frame(origin: DVec3, z: DVec3, x_hint: DVec3) -> Result<DMat4, String> {
  let z = z.try_normalize().ok_or("a joint axis has no direction")?;
  let mut x = x_hint - z * x_hint.dot(z);
  if x.length_squared() < 1e-12 {
    let a = if z.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    x = a - z * a.dot(z);
  }
  let x = x.normalize();
  let y = z.cross(x);
  Ok(DMat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), origin.extend(1.0)))
}

/// A world-aligned X axis for a Z axis (as for sketches on faces: X right, Y up).
fn x_for(z: DVec3) -> DVec3 {
  if z.z.abs() > 0.9 { DVec3::X } else { DVec3::Z.cross(z).normalize_or_zero() }
}

/// An axis line's direction with a fixed sense: +Z first, then +Y, then +X, so a cylinder gives
/// the same joint axis however its surface happens to be parametrised.
fn canonical_axis(a: DVec3) -> DVec3 {
  const EPS: f64 = 1e-9;
  let lead = if a.z.abs() > EPS {
    a.z
  } else if a.y.abs() > EPS {
    a.y
  } else {
    a.x
  };
  if lead < 0.0 { -a } else { a }
}

/// Frame of a face joint origin.
pub fn face_frame(f: &FaceInfo) -> Result<DMat4, String> {
  match f.kind {
    SurfaceKind::Plane => frame(f.center, f.normal, x_for(f.normal)),
    SurfaceKind::Cylinder => {
      // on the axis, level with the face's centroid; OCC's axis sense depends on how the surface
      // was parametrised, so give it a fixed sense (+Z for an upright cylinder, as in Fusion)
      let a = canonical_axis(f.normal.normalize_or_zero());
      let o = f.origin + a * (f.center - f.origin).dot(a);
      frame(o, a, x_for(a))
    }
    SurfaceKind::Sphere => frame(f.origin, DVec3::Z, DVec3::X),
    _ => frame(f.center, f.normal, x_for(f.normal)),
  }
}

/// Frame of a circular edge: centre, axis out of the adjacent flat face.
pub fn edge_frame(e: &EdgeInfo, faces: &[FaceInfo]) -> Result<DMat4, String> {
  if e.kind != CurveKind::Circle {
    // straight edges: the midpoint, Z along the edge
    return frame(e.mid, e.end - e.start, x_for(e.end - e.start));
  }
  let mut z = e.axis.normalize_or_zero();
  if let Some(f) =
    faces.iter().find(|f| e.faces.contains(&f.id) && f.kind == SurfaceKind::Plane && f.normal.dot(z).abs() > 0.999)
  {
    z *= f.normal.dot(z).signum();
  }
  frame(e.center, z, x_for(z))
}

/// The motion of a joint in its own frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Motion {
  pub rotation: f64,
  pub slide: f64,
  pub slide2: f64,
  pub pitch: f64,
}

impl Motion {
  /// The motion as a matrix, keeping only what `kind` allows.
  pub fn matrix(&self, kind: JointKind) -> DMat4 {
    let (rot, sl, sl2, pitch) = kind.freedoms();
    let along = match kind {
      JointKind::PinSlot | JointKind::Planar => DVec3::new(self.slide, if sl2 { self.slide2 } else { 0.0 }, 0.0),
      _ => DVec3::new(0.0, 0.0, if sl { self.slide } else { 0.0 }),
    };
    DMat4::from_translation(along)
      * DMat4::from_rotation_z(if rot { self.rotation } else { 0.0 })
      * DMat4::from_rotation_x(if pitch { self.pitch } else { 0.0 })
  }
}

/// How far a screw joint advances along its axis for a rotation (rad), given its lead (the
/// advance per full turn; negative = left-handed).
pub fn screw_slide(rotation: f64, lead: f64) -> f64 {
  rotation / std::f64::consts::TAU * lead
}

/// Where the moving component goes: the transform taking its bodies from where they are to their
/// jointed position.
pub fn joint_transform(
  fixed: DMat4,
  moving: DMat4,
  kind: JointKind,
  angle: f64,
  offset: f64,
  flip: bool,
  as_built: bool,
  motion: Motion,
) -> DMat4 {
  let m = motion.matrix(kind);
  if as_built {
    return fixed * m * fixed.inverse();
  }
  // Z axes face each other unless flipped: turn the moving frame half a turn about its X
  let mate = if flip { DMat4::IDENTITY } else { DMat4::from_rotation_x(std::f64::consts::PI) };
  fixed
    * DMat4::from_translation(DVec3::new(0.0, 0.0, offset))
    * DMat4::from_rotation_z(angle)
    * m
    * mate
    * moving.inverse()
}

/// One of a joint's motions, as motion links read and drive them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MotionAxis {
  #[default]
  Rotation,
  Slide,
  Slide2,
  Pitch,
}

impl MotionAxis {
  pub const ALL: [MotionAxis; 4] = [MotionAxis::Rotation, MotionAxis::Slide, MotionAxis::Slide2, MotionAxis::Pitch];

  pub fn name(self) -> &'static str {
    match self {
      MotionAxis::Rotation => "rotation",
      MotionAxis::Slide => "slide",
      MotionAxis::Slide2 => "slide2",
      MotionAxis::Pitch => "pitch",
    }
  }

  pub fn parse(s: &str) -> Option<MotionAxis> {
    MotionAxis::ALL.into_iter().find(|a| a.name() == s)
  }

  /// Angles (rotation, pitch) or lengths (slides).
  pub fn is_angle(self) -> bool {
    matches!(self, MotionAxis::Rotation | MotionAxis::Pitch)
  }

  /// Does a joint of this kind have this motion?
  pub fn allowed(self, kind: JointKind) -> bool {
    let (rot, sl, sl2, pitch) = kind.freedoms();
    match self {
      MotionAxis::Rotation => rot,
      // a screw's slide isn't free: its rotation sets it
      MotionAxis::Slide => sl && kind != JointKind::Screw,
      MotionAxis::Slide2 => sl2,
      MotionAxis::Pitch => pitch,
    }
  }

  /// The first motion a joint of this kind has.
  pub fn first(kind: JointKind) -> Option<MotionAxis> {
    MotionAxis::ALL.into_iter().find(|a| a.allowed(kind))
  }

  pub fn get(self, m: &Motion) -> f64 {
    match self {
      MotionAxis::Rotation => m.rotation,
      MotionAxis::Slide => m.slide,
      MotionAxis::Slide2 => m.slide2,
      MotionAxis::Pitch => m.pitch,
    }
  }

  pub fn set(self, m: &mut Motion, v: f64) {
    match self {
      MotionAxis::Rotation => m.rotation = v,
      MotionAxis::Slide => m.slide = v,
      MotionAxis::Slide2 => m.slide2 = v,
      MotionAxis::Pitch => m.pitch = v,
    }
  }
}

/// The data hub design (and version) an inserted component was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HubSource {
  /// Hub item id.
  pub item: String,
  pub version: u32,
}

/// Components whose bodies can't pass through each other while joints are driven: driving
/// stops where they first touch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactSet {
  pub id: u32,
  pub name: String,
  #[serde(default)]
  pub components: Vec<ComponentId>,
  #[serde(default = "yes")]
  pub enabled: bool,
}

fn yes() -> bool {
  true
}

/// What an align constraint makes of its two geometries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlignRelation {
  /// Flush for two flat faces, concentric for two axes (cylinders, edges), coincident otherwise.
  #[default]
  Auto,
  /// Planes: the moving plane lies on the fixed one (offset along its normal), normals opposed.
  Flush,
  /// Points: the moving point lies on the fixed one (offset along the fixed Z axis).
  Coincident,
  /// Axes: the moving axis lies on the fixed one.
  Concentric,
}

/// The kind of geometry an align constraint picked: a flat face, an axis or a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignShape {
  Plane,
  Axis,
  Point,
}

impl AlignRelation {
  pub fn parse(s: &str) -> Option<AlignRelation> {
    match s {
      "auto" => Some(AlignRelation::Auto),
      "flush" => Some(AlignRelation::Flush),
      "coincident" => Some(AlignRelation::Coincident),
      "concentric" => Some(AlignRelation::Concentric),
      _ => None,
    }
  }

  /// The relation `Auto` stands for, given the two geometries.
  pub fn resolve(self, a: AlignShape, b: AlignShape) -> AlignRelation {
    match (self, a, b) {
      (AlignRelation::Auto, AlignShape::Plane, AlignShape::Plane) => AlignRelation::Flush,
      (AlignRelation::Auto, AlignShape::Axis, AlignShape::Axis) => AlignRelation::Concentric,
      (AlignRelation::Auto, _, _) => AlignRelation::Coincident,
      (r, _, _) => r,
    }
  }
}

/// Where an align constraint moves the moving component. Unlike a joint, it removes only the
/// freedoms its relation fixes and moves the component as little as it can, so what the relation
/// leaves free stays where it was (Fusion's assembly constraints).
///
/// - Flush: turns the moving plane about its origin, by the smallest rotation, until its normal
///   opposes the fixed one (or matches it with `flip`), then slides it along that normal to
///   `offset` from the fixed plane.
/// - Concentric: turns the moving axis parallel to the fixed one (the nearer way, the other with
///   `flip`), then slides it across onto the fixed axis, keeping its position along the axis.
/// - Coincident: translates the moving point onto the fixed one, `offset` along the fixed Z.
pub fn align_transform(
  fixed: DMat4,
  moving: DMat4,
  relation: AlignRelation,
  offset: f64,
  flip: bool,
) -> Result<DMat4, String> {
  let axis = |m: DMat4| m.z_axis.truncate().try_normalize().ok_or("an align axis has no direction");
  let (fo, fz) = (fixed.w_axis.truncate(), axis(fixed)?);
  let (mo, mz) = (moving.w_axis.truncate(), axis(moving)?);
  // the smallest turn taking `from` to `to`, about the moving geometry's origin
  let turn = |from: DVec3, to: DVec3| {
    DMat4::from_translation(mo) * DMat4::from_quat(DQuat::from_rotation_arc(from, to)) * DMat4::from_translation(-mo)
  };
  Ok(match relation {
    AlignRelation::Flush | AlignRelation::Auto => {
      let r = turn(mz, if flip { fz } else { -fz });
      DMat4::from_translation(fz * (fo + fz * offset - mo).dot(fz)) * r
    }
    AlignRelation::Concentric => {
      if offset.abs() > 1e-12 {
        return Err("a concentric alignment takes no offset".into());
      }
      let same = mz.dot(fz) >= 0.0;
      let r = turn(mz, if same != flip { fz } else { -fz });
      let d = fo - mo;
      DMat4::from_translation(d - fz * d.dot(fz)) * r
    }
    AlignRelation::Coincident => DMat4::from_translation(fo + fz * offset - mo),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn align_moves_only_what_the_relation_fixes() {
    // a block's bottom (at (55, 5, 0), facing down) flush 5 mm above a plate's top (z 10)
    let fixed = frame(DVec3::new(20.0, 15.0, 10.0), DVec3::Z, DVec3::X).unwrap();
    let moving = frame(DVec3::new(55.0, 5.0, 0.0), -DVec3::Z, DVec3::X).unwrap();
    let t = align_transform(fixed, moving, AlignRelation::Flush, 5.0, false).unwrap();
    assert!(close(t.transform_point3(DVec3::new(60.0, 10.0, 0.0)), DVec3::new(60.0, 10.0, 15.0)));
    // a face turned on its side is turned down onto the plane about its own centre
    let moving = frame(DVec3::new(55.0, 5.0, 0.0), DVec3::X, DVec3::Y).unwrap();
    let t = align_transform(fixed, moving, AlignRelation::Flush, 0.0, false).unwrap();
    assert!(close(t.transform_point3(DVec3::new(55.0, 5.0, 0.0)), DVec3::new(55.0, 5.0, 10.0)));
    assert!(close(t.transform_vector3(DVec3::X), -DVec3::Z));
    // concentric keeps the axial position; coincident snaps the points together
    let fixed = frame(DVec3::ZERO, DVec3::Z, DVec3::X).unwrap();
    let moving = frame(DVec3::new(3.0, 4.0, 7.0), -DVec3::Z, DVec3::X).unwrap();
    let t = align_transform(fixed, moving, AlignRelation::Concentric, 0.0, false).unwrap();
    assert!(close(t.transform_point3(DVec3::new(3.0, 4.0, 9.0)), DVec3::new(0.0, 0.0, 9.0)));
    let t = align_transform(fixed, moving, AlignRelation::Coincident, 1.0, false).unwrap();
    assert!(close(t.transform_point3(DVec3::new(3.0, 4.0, 7.0)), DVec3::new(0.0, 0.0, 1.0)));
  }

  fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
  }

  #[test]
  fn mating_puts_origins_together_facing() {
    // a pin's bottom (z out = −Z, at (5, 5, 0)) into a hole's top (z out = +Z, at (0, 0, 10))
    let fixed = frame(DVec3::new(0.0, 0.0, 10.0), DVec3::Z, DVec3::X).unwrap();
    let moving = frame(DVec3::new(5.0, 5.0, 0.0), -DVec3::Z, DVec3::X).unwrap();
    let t = joint_transform(fixed, moving, JointKind::Rigid, 0.0, 0.0, false, false, Motion::default());
    assert!(close(t.transform_point3(DVec3::new(5.0, 5.0, 0.0)), DVec3::new(0.0, 0.0, 10.0)));
    // the pin keeps pointing up
    assert!(close(t.transform_point3(DVec3::new(5.0, 5.0, 7.0)), DVec3::new(0.0, 0.0, 17.0)));
    // revolute motion spins about the joint axis; the offset lifts along it
    let t = joint_transform(
      fixed,
      moving,
      JointKind::Revolute,
      0.0,
      2.0,
      false,
      false,
      Motion { rotation: std::f64::consts::FRAC_PI_2, ..Default::default() },
    );
    assert!(close(t.transform_point3(DVec3::new(6.0, 5.0, 0.0)), DVec3::new(0.0, 1.0, 12.0)));
  }

  #[test]
  fn slider_ignores_rotation() {
    let f = frame(DVec3::ZERO, DVec3::X, DVec3::Y).unwrap();
    let m = Motion { rotation: 1.0, slide: 3.0, ..Default::default() };
    let t = joint_transform(f, f, JointKind::Slider, 0.0, 0.0, true, true, m);
    assert!(close(t.transform_point3(DVec3::ZERO), DVec3::new(3.0, 0.0, 0.0)));
    assert!(close(t.transform_point3(DVec3::Y), DVec3::new(3.0, 1.0, 0.0)));
  }
}
