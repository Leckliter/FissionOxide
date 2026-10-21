//! The Render workspace's data in a design: physically based materials on bodies and faces, the
//! environment, the render camera and settings, and the gallery of finished renders. The types
//! themselves (materials, environments, the tracer) live in `fox-raytrace`.

use fox_raytrace::{Environment, GalleryImage, Material, RenderCamera, RenderSettings};
use serde::{Deserialize, Serialize};

use crate::{BodyId, FaceRef};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderDoc {
  /// Materials made in (or copied into) this design, beside the library's.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub materials: Vec<Material>,
  /// Material of whole bodies, by name.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub bodies: Vec<BodyMaterial>,
  /// Material of single faces, over their body's.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub faces: Vec<FaceMaterial>,
  #[serde(skip_serializing_if = "is_default")]
  pub environment: Environment,
  /// The camera renders are taken from (`None` = the viewport's).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub camera: Option<RenderCamera>,
  #[serde(skip_serializing_if = "is_default")]
  pub settings: RenderSettings,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub gallery: Vec<GalleryImage>,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
  *v == T::default()
}

impl RenderDoc {
  pub fn is_empty(&self) -> bool {
    *self == RenderDoc::default()
  }

  /// A material by name: the design's own first, then the library's.
  pub fn material(&self, name: &str) -> Option<Material> {
    self
      .materials
      .iter()
      .find(|m| m.name.eq_ignore_ascii_case(name))
      .cloned()
      .or_else(|| fox_raytrace::material::find(name))
  }

  /// The material name given to a body, if any.
  pub fn body_material(&self, body: BodyId) -> Option<&str> {
    self.bodies.iter().find(|b| b.body == body).map(|b| b.material.as_str())
  }

  pub fn next_gallery_id(&self) -> u32 {
    self.gallery.iter().map(|g| g.id).max().unwrap_or(0) + 1
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyMaterial {
  pub body: BodyId,
  pub material: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceMaterial {
  pub face: FaceRef,
  pub material: String,
}
