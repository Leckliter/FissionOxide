//! Assemblies: components (named groups of bodies) and the joints that position them.
//!
//! A joint mates a joint origin on the moving component to one on the fixed component (or the
//! world): the origins' points coincide and their Z axes face each other (so faces mate), then
//! the joint's own motion — rotation about Z, slides along Z or X/Y, tilt — moves the component
//! within the degrees of freedom of the joint type. Motion values are expressions, so parameters
//! drive mechanisms and limits are checked on every rebuild.

use fox_kernel::glam::{DMat4, DVec3};
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

/// Frame of a face joint origin.
pub fn face_frame(f: &FaceInfo) -> Result<DMat4, String> {
  match f.kind {
    SurfaceKind::Plane => frame(f.center, f.normal, x_for(f.normal)),
    SurfaceKind::Cylinder => {
      // on the axis, level with the face's centroid
      let a = f.normal.normalize_or_zero();
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
  // As in Fusion, the moving origin's axes line up with the fixed origin's (Z the same way);
  // flipping turns the moving frame half a turn about its X so the Z axes face each other
  let mate = if flip { DMat4::from_rotation_x(std::f64::consts::PI) } else { DMat4::IDENTITY };
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

#[cfg(test)]
mod tests {
  use super::*;

  fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
  }

  #[test]
  fn mating_puts_origins_together_facing() {
    // a pin's bottom (z out = −Z, at (5, 5, 0)) into a hole's top (z out = +Z, at (0, 0, 10))
    let fixed = frame(DVec3::new(0.0, 0.0, 10.0), DVec3::Z, DVec3::X).unwrap();
    let moving = frame(DVec3::new(5.0, 5.0, 0.0), -DVec3::Z, DVec3::X).unwrap();
    // unflipped, the Z axes line up: the pin's −Z bottom normal turns to +Z, so it hangs down
    let t = joint_transform(fixed, moving, JointKind::Rigid, 0.0, 0.0, false, false, Motion::default());
    assert!(close(t.transform_point3(DVec3::new(5.0, 5.0, 0.0)), DVec3::new(0.0, 0.0, 10.0)));
    assert!(close(t.transform_point3(DVec3::new(5.0, 5.0, 7.0)), DVec3::new(0.0, 0.0, 3.0)));
    // flipped, the Z axes face each other and the pin keeps pointing up
    let t = joint_transform(fixed, moving, JointKind::Rigid, 0.0, 0.0, true, false, Motion::default());
    assert!(close(t.transform_point3(DVec3::new(5.0, 5.0, 0.0)), DVec3::new(0.0, 0.0, 10.0)));
    assert!(close(t.transform_point3(DVec3::new(5.0, 5.0, 7.0)), DVec3::new(0.0, 0.0, 17.0)));
    // revolute motion spins about the joint axis; the offset lifts along it
    let t = joint_transform(
      fixed,
      moving,
      JointKind::Revolute,
      0.0,
      2.0,
      true,
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
