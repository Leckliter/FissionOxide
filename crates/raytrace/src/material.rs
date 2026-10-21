//! Physically based materials (metallic/roughness with clearcoat, transmission and emission) and
//! the library they come from. The library holds every Design-workspace appearance under the
//! same name and colour, so a body painted "Brass - Polished" renders as polished brass.

use serde::{Deserialize, Serialize};

/// A physically based material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
  pub name: String,
  /// Library section (Metal, Plastic, Glass, ...).
  pub category: String,
  /// Base colour (sRGB): the albedo of dielectrics, the reflectance of metals.
  pub color: [u8; 3],
  /// 0 = dielectric, 1 = metal.
  pub metallic: f32,
  /// Microfacet roughness, 0 (mirror) to 1.
  pub roughness: f32,
  /// Weight of a glossy clear layer over the base (car paint, varnish).
  pub clearcoat: f32,
  pub clearcoat_roughness: f32,
  /// 0 = opaque, 1 = fully transmissive (glass, clear plastic).
  pub transmission: f32,
  /// Index of refraction of dielectrics (glass 1.5).
  pub ior: f32,
  /// Emitted colour (sRGB) and its strength (0 = none).
  pub emission: [u8; 3],
  pub emission_strength: f32,
  pub texture: Option<TextureSpec>,
}

impl Default for Material {
  fn default() -> Self {
    Material {
      name: String::new(),
      category: "Custom".into(),
      color: [200, 200, 200],
      metallic: 0.0,
      roughness: 0.5,
      clearcoat: 0.0,
      clearcoat_roughness: 0.05,
      transmission: 0.0,
      ior: 1.5,
      emission: [255, 255, 255],
      emission_strength: 0.0,
      texture: None,
    }
  }
}

/// A texture on a material: an image or a built-in pattern, projected onto faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureSpec {
  pub source: TextureSource,
  /// The pattern's second colour (sRGB); a pattern blends from the material's colour to this.
  pub color2: [u8; 3],
  pub projection: Projection,
  /// Axis of planar and cylindrical projections.
  pub axis: Axis,
  /// Size of one repeat of the texture (mm).
  pub scale: f32,
  /// Rotation of the texture on the surface (degrees).
  pub rotation: f32,
  /// Shift of the texture, in repeats.
  pub offset: [f32; 2],
  /// How much the texture's brightness roughens the surface (0 = not at all).
  pub roughness_variation: f32,
}

impl Default for TextureSpec {
  fn default() -> Self {
    TextureSpec {
      source: TextureSource::Pattern(Pattern::Checker),
      color2: [40, 40, 40],
      projection: Projection::Box,
      axis: Axis::Z,
      scale: 20.0,
      rotation: 0.0,
      offset: [0.0, 0.0],
      roughness_variation: 0.0,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureSource {
  /// An image file (PNG, JPEG, ...), tinted by the material's colour.
  Image(String),
  Pattern(Pattern),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
  Checker,
  Wood,
  Brushed,
  Carbon,
  Noise,
  Grid,
  Knurl,
}

impl Pattern {
  pub const ALL: [Pattern; 7] =
    [Pattern::Checker, Pattern::Wood, Pattern::Brushed, Pattern::Carbon, Pattern::Noise, Pattern::Grid, Pattern::Knurl];

  pub fn name(self) -> &'static str {
    match self {
      Pattern::Checker => "checker",
      Pattern::Wood => "wood",
      Pattern::Brushed => "brushed",
      Pattern::Carbon => "carbon",
      Pattern::Noise => "noise",
      Pattern::Grid => "grid",
      Pattern::Knurl => "knurl",
    }
  }

  pub fn parse(s: &str) -> Option<Pattern> {
    Pattern::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s))
  }
}

/// How texture coordinates come from a surface point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
  /// Planar along whichever axis the face's normal is closest to (Fusion's Automatic/box).
  #[default]
  Box,
  Planar,
  Cylindrical,
  Spherical,
}

impl Projection {
  pub fn parse(s: &str) -> Option<Projection> {
    Some(match s.to_ascii_lowercase().as_str() {
      "box" | "automatic" | "auto" | "triplanar" => Projection::Box,
      "planar" | "plane" => Projection::Planar,
      "cylindrical" | "cylinder" => Projection::Cylindrical,
      "spherical" | "sphere" => Projection::Spherical,
      _ => return None,
    })
  }

  pub fn index(self) -> i32 {
    self as i32
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
  X,
  Y,
  #[default]
  Z,
}

impl Axis {
  pub fn parse(s: &str) -> Option<Axis> {
    Some(match s.to_ascii_lowercase().as_str() {
      "x" => Axis::X,
      "y" => Axis::Y,
      "z" => Axis::Z,
      _ => return None,
    })
  }

  pub fn index(self) -> usize {
    self as usize
  }
}

fn m(name: &str, category: &str, color: [u8; 3], metallic: f32, roughness: f32) -> Material {
  Material { name: name.into(), category: category.into(), color, metallic, roughness, ..Material::default() }
}

fn textured(mut mat: Material, pattern: Pattern, color2: [u8; 3], scale: f32, variation: f32) -> Material {
  mat.texture = Some(TextureSpec {
    source: TextureSource::Pattern(pattern),
    color2,
    scale,
    roughness_variation: variation,
    ..TextureSpec::default()
  });
  mat
}

fn coat(mut mat: Material, clearcoat: f32, roughness: f32) -> Material {
  mat.clearcoat = clearcoat;
  mat.clearcoat_roughness = roughness;
  mat
}

fn glass(name: &str, color: [u8; 3], roughness: f32, ior: f32) -> Material {
  Material { transmission: 1.0, ior, ..m(name, "Glass", color, 0.0, roughness) }
}

fn emissive(name: &str, color: [u8; 3], strength: f32) -> Material {
  Material { emission: color, emission_strength: strength, ..m(name, "Emissive", color, 0.0, 0.4) }
}

/// The material library: the Design workspace's appearances first (same names and colours),
/// then more metals, plastics, paints, glass, wood and lights.
pub fn library() -> Vec<Material> {
  vec![
    // the appearances (see fox-engine's APPEARANCES)
    m("Steel - Satin", "Metal", [186, 190, 196], 1.0, 0.32),
    coat(m("Aluminum - Anodized Black", "Metal", [52, 54, 58], 1.0, 0.38), 0.2, 0.2),
    coat(m("Aluminum - Anodized Red", "Metal", [176, 46, 44], 1.0, 0.35), 0.2, 0.2),
    coat(m("Aluminum - Anodized Blue", "Metal", [46, 92, 170], 1.0, 0.35), 0.2, 0.2),
    textured(
      m("Aluminum - Brushed", "Metal", [205, 207, 210], 1.0, 0.28),
      Pattern::Brushed,
      [170, 172, 176],
      30.0,
      0.25,
    ),
    m("Brass - Polished", "Metal", [204, 170, 84], 1.0, 0.08),
    m("Copper", "Metal", [190, 112, 72], 1.0, 0.22),
    m("Gold - Polished", "Metal", [222, 182, 70], 1.0, 0.06),
    m("Plastic - Matte (White)", "Plastic", [236, 236, 232], 0.0, 0.65),
    m("Plastic - Matte (Black)", "Plastic", [40, 40, 42], 0.0, 0.6),
    m("ABS (Red)", "Plastic", [208, 50, 46], 0.0, 0.35),
    m("ABS (Yellow)", "Plastic", [236, 196, 48], 0.0, 0.35),
    m("ABS (Green)", "Plastic", [70, 160, 80], 0.0, 0.35),
    m("Rubber - Black", "Plastic", [30, 30, 30], 0.0, 0.85),
    coat(
      textured(m("Wood - Oak", "Wood", [180, 132, 82], 0.0, 0.55), Pattern::Wood, [128, 86, 48], 40.0, 0.2),
      0.3,
      0.15,
    ),
    glass("Glass - Clear", [190, 220, 230], 0.0, 1.5),
    // more metals
    m("Steel - Polished", "Metal", [196, 199, 204], 1.0, 0.05),
    textured(m("Steel - Brushed", "Metal", [180, 184, 190], 1.0, 0.3), Pattern::Brushed, [150, 154, 160], 30.0, 0.25),
    m("Stainless Steel - Satin", "Metal", [200, 202, 205], 1.0, 0.25),
    m("Chrome", "Metal", [230, 232, 236], 1.0, 0.02),
    m("Aluminum - Polished", "Metal", [228, 230, 232], 1.0, 0.06),
    m("Aluminum - Satin", "Metal", [214, 216, 219], 1.0, 0.35),
    m("Titanium - Satin", "Metal", [170, 166, 160], 1.0, 0.3),
    m("Iron - Cast", "Metal", [110, 110, 112], 1.0, 0.7),
    m("Copper - Polished", "Metal", [214, 138, 96], 1.0, 0.06),
    m("Bronze - Satin", "Metal", [176, 128, 70], 1.0, 0.35),
    textured(m("Steel - Knurled", "Metal", [186, 190, 196], 1.0, 0.3), Pattern::Knurl, [120, 124, 130], 3.0, 0.3),
    // plastics
    m("Plastic - Glossy (White)", "Plastic", [240, 240, 238], 0.0, 0.12),
    m("Plastic - Glossy (Black)", "Plastic", [24, 24, 26], 0.0, 0.12),
    m("Plastic - Glossy (Red)", "Plastic", [200, 30, 30], 0.0, 0.12),
    m("Plastic - Glossy (Blue)", "Plastic", [30, 80, 190], 0.0, 0.12),
    m("Plastic - Matte (Grey)", "Plastic", [128, 130, 134], 0.0, 0.6),
    m("Nylon - White", "Plastic", [232, 228, 216], 0.0, 0.75),
    textured(
      m("Plastic - Textured (Black)", "Plastic", [44, 44, 46], 0.0, 0.7),
      Pattern::Noise,
      [28, 28, 30],
      4.0,
      0.3,
    ),
    Material { transmission: 0.9, ior: 1.49, ..m("Acrylic - Clear", "Plastic", [235, 240, 245], 0.0, 0.03) },
    Material { transmission: 0.85, ior: 1.46, ..m("Polycarbonate - Smoke", "Plastic", [90, 92, 96], 0.0, 0.05) },
    m("Rubber - Grey", "Plastic", [92, 94, 96], 0.0, 0.9),
    // paint
    coat(m("Paint - Metallic Red", "Paint", [150, 16, 20], 0.6, 0.4), 1.0, 0.03),
    coat(m("Paint - Metallic Blue", "Paint", [20, 50, 130], 0.6, 0.4), 1.0, 0.03),
    coat(m("Paint - Gloss White", "Paint", [238, 238, 236], 0.0, 0.4), 1.0, 0.03),
    coat(m("Paint - Gloss Black", "Paint", [16, 16, 18], 0.0, 0.4), 1.0, 0.03),
    m("Powder Coat - Black", "Paint", [34, 34, 36], 0.0, 0.5),
    // glass and liquids
    glass("Glass - Frosted", [236, 240, 242], 0.35, 1.5),
    glass("Glass - Tinted Blue", [110, 160, 210], 0.0, 1.5),
    glass("Glass - Tinted Green", [120, 190, 140], 0.0, 1.5),
    glass("Water", [230, 240, 245], 0.0, 1.33),
    glass("Diamond", [250, 250, 252], 0.0, 2.42),
    // composites, wood, stone
    coat(
      textured(m("Carbon Fiber", "Other", [26, 26, 28], 0.2, 0.4), Pattern::Carbon, [70, 72, 76], 8.0, 0.2),
      1.0,
      0.04,
    ),
    coat(
      textured(m("Wood - Walnut", "Wood", [110, 70, 42], 0.0, 0.5), Pattern::Wood, [70, 42, 24], 40.0, 0.2),
      0.3,
      0.15,
    ),
    textured(m("Wood - Pine", "Wood", [214, 176, 120], 0.0, 0.6), Pattern::Wood, [176, 128, 74], 50.0, 0.2),
    textured(m("Concrete", "Other", [150, 148, 144], 0.0, 0.9), Pattern::Noise, [118, 116, 112], 25.0, 0.1),
    m("Ceramic - White", "Other", [244, 244, 240], 0.0, 0.08),
    m("Leather - Brown", "Other", [92, 56, 36], 0.0, 0.7),
    textured(m("Checker (test)", "Other", [230, 230, 230], 0.0, 0.5), Pattern::Checker, [40, 40, 40], 20.0, 0.0),
    // lights
    emissive("LED - White", [255, 250, 240], 8.0),
    emissive("LED - Red", [255, 40, 30], 8.0),
    emissive("LED - Blue", [40, 110, 255], 8.0),
  ]
}

/// A library material by name (case-insensitive).
pub fn find(name: &str) -> Option<Material> {
  library().into_iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// What a body coloured `color` (and given no material) renders as: the library material with
/// that exact colour (an appearance), else a satin plastic of that colour.
pub fn for_color(color: [u8; 3]) -> Material {
  library().into_iter().take(16).find(|m| m.color == color).unwrap_or_else(|| Material {
    name: "Plastic".into(),
    color,
    roughness: 0.4,
    ..Material::default()
  })
}

/// What a body with no appearance renders as (Fusion's default, Steel - Satin).
pub fn default_material() -> Material {
  library().remove(0)
}
