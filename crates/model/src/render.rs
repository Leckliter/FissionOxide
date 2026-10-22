//! The Render workspace's data in a design: physically based materials on bodies and faces, the
//! environment, the render camera and settings, and the gallery of finished renders. The types
//! themselves (materials, environments, the tracer) live in `fox-raytrace`.

use fox_raytrace::{Environment, GalleryImage, Light, Material, RenderCamera, RenderSettings};
use serde::{Deserialize, Serialize};

use crate::{BodyId, FaceRef};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
  /// A file without one kept [`Environment::default`]; a new design starts with
  /// [`Environment::new_design`] (and so writes it).
  #[serde(default, skip_serializing_if = "is_default")]
  pub environment: Environment,
  /// The camera renders are taken from (`None` = the viewport's).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub camera: Option<RenderCamera>,
  #[serde(skip_serializing_if = "is_default")]
  pub settings: RenderSettings,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub gallery: Vec<GalleryImage>,
  /// Lights placed in the scene (at most [`fox_raytrace::light::MAX_LIGHTS`]).
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub lights: Vec<Light>,
  /// Dielectric priorities of bodies (bodies not listed have 0).
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub priorities: Vec<BodyPriority>,
}

impl Default for RenderDoc {
  /// A new design's: no materials, lights or renders, Fusion's grey scene.
  fn default() -> Self {
    RenderDoc { environment: Environment::new_design(), ..RenderDoc::saved() }
  }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
  *v == T::default()
}

impl RenderDoc {
  /// What a file without render data had: nothing set, the environment as the background.
  pub fn saved() -> RenderDoc {
    RenderDoc {
      materials: Vec::new(),
      bodies: Vec::new(),
      faces: Vec::new(),
      environment: Environment::default(),
      camera: None,
      settings: RenderSettings::default(),
      gallery: Vec::new(),
      lights: Vec::new(),
      priorities: Vec::new(),
    }
  }

  /// Nothing a file needs to keep: [`RenderDoc::saved`], which a file without render data reads
  /// back as.
  pub fn is_empty(&self) -> bool {
    *self == RenderDoc::saved()
  }

  /// A body's dielectric priority.
  pub fn priority(&self, body: BodyId) -> u8 {
    self.priorities.iter().find(|p| p.body == body).map_or(0, |p| p.priority)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyPriority {
  pub body: BodyId,
  pub priority: u8,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_new_designs_grey_scene_is_kept_and_an_old_files_environment_is_not_changed() {
    let new = RenderDoc::default();
    assert!(!new.is_empty(), "the grey background is written");
    let back: RenderDoc = serde_json::from_value(serde_json::to_value(&new).unwrap()).unwrap();
    assert_eq!(back, new);
    // a file that left the environment out had the environment as its background
    let old: RenderDoc = serde_json::from_str("{}").unwrap();
    assert_eq!(old.environment, Environment::default());
    assert!(old.is_empty());
  }
}
