//! The render camera: the viewport camera's framing plus a lens (depth of field) and exposure.

use fox_render::Camera;
use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::sampling::{Rng, concentric_disc};

/// Height of the sensor the field of view is measured on (35 mm film), for the lens maths.
const SENSOR_HEIGHT: f64 = 24.0;

/// The camera a render is taken with, stored with the design.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderCamera {
  /// Framing (orbit target, angles, distance, field of view, orthographic).
  pub view: Camera,
  /// Aperture as an f-number; 0 = a pinhole (everything sharp).
  pub fstop: f64,
  /// Distance to the plane in focus (mm); `None` = the orbit target.
  pub focus_distance: Option<f64>,
  /// Exposure compensation in stops.
  pub exposure: f32,
  pub tone_map: ToneMap,
}

impl Default for RenderCamera {
  fn default() -> Self {
    RenderCamera { view: Camera::default(), fstop: 0.0, focus_distance: None, exposure: 0.0, tone_map: ToneMap::Aces }
  }
}

/// How scene radiance becomes display colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToneMap {
  /// ACES filmic curve (soft highlights).
  #[default]
  Aces,
  /// Reinhard.
  Reinhard,
  /// Clamp.
  Linear,
}

impl ToneMap {
  pub fn parse(s: &str) -> Option<ToneMap> {
    Some(match s.to_ascii_lowercase().as_str() {
      "aces" | "filmic" => ToneMap::Aces,
      "reinhard" => ToneMap::Reinhard,
      "linear" | "none" | "clamp" => ToneMap::Linear,
      _ => return None,
    })
  }

  pub fn apply(self, c: Vec3) -> Vec3 {
    match self {
      ToneMap::Aces => {
        // Narkowicz's fit
        let (a, b, cc, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
        ((c * (c * a + b)) / (c * (c * cc + d) + e)).clamp(Vec3::ZERO, Vec3::ONE)
      }
      ToneMap::Reinhard => c / (c + Vec3::ONE),
      ToneMap::Linear => c.clamp(Vec3::ZERO, Vec3::ONE),
    }
  }

  pub fn index(self) -> i32 {
    self as i32
  }
}

/// A camera ready to shoot rays for an image of `width` × `height`.
#[derive(Debug, Clone)]
pub struct Lens {
  eye: Vec3,
  right: Vec3,
  up: Vec3,
  forward: Vec3,
  /// Half extents of the image plane at distance 1 (perspective) or in mm (orthographic).
  half_w: f32,
  half_h: f32,
  ortho: bool,
  lens_radius: f32,
  focus: f32,
  width: f32,
  height: f32,
}

impl Lens {
  pub fn new(cam: &RenderCamera, width: u32, height: u32) -> Lens {
    let v = &cam.view;
    let (right, up, back) = v.basis();
    let aspect = width as f64 / height.max(1) as f64;
    let half_h = if v.ortho { v.view_height() / 2.0 } else { (v.fov / 2.0).tan() };
    let focus = cam.focus_distance.unwrap_or(v.distance).max(1e-3);
    // f = (sensor / 2) / tan(fov / 2); aperture diameter = f / N
    let focal = SENSOR_HEIGHT / 2.0 / (v.fov / 2.0).tan().max(1e-6);
    let lens_radius = if cam.fstop > 0.0 && !v.ortho { focal / cam.fstop / 2.0 } else { 0.0 };
    let f = |d: DVec3| d.as_vec3();
    Lens {
      eye: f(v.eye()),
      right: f(right),
      up: f(up),
      forward: f(-back),
      half_w: (half_h * aspect) as f32,
      half_h: half_h as f32,
      ortho: v.ortho,
      lens_radius: lens_radius as f32,
      focus: focus as f32,
      width: width as f32,
      height: height as f32,
    }
  }

  /// The ray through image point (`px`, `py`) (pixels, top-left origin): origin and direction.
  pub fn ray(&self, px: f32, py: f32, rng: &mut Rng) -> (Vec3, Vec3) {
    let sx = (px / self.width * 2.0 - 1.0) * self.half_w;
    let sy = (1.0 - py / self.height * 2.0) * self.half_h;
    if self.ortho {
      // start well behind the target so nothing in front of the model is cut away
      let o = self.eye + self.right * sx + self.up * sy - self.forward * 1e4;
      return (o, self.forward);
    }
    let d = (self.forward + self.right * sx + self.up * sy).normalize();
    if self.lens_radius <= 0.0 {
      return (self.eye, d);
    }
    // thin lens: every ray through the lens meets at the focus plane
    let t = self.focus / d.dot(self.forward);
    let focus_pt = self.eye + d * t;
    let (lx, ly) = concentric_disc(rng.f32(), rng.f32());
    let o = self.eye + (self.right * lx + self.up * ly) * self.lens_radius;
    (o, (focus_pt - o).normalize())
  }

  /// Direction of the view (for backgrounds).
  pub fn forward(&self) -> Vec3 {
    self.forward
  }
}
