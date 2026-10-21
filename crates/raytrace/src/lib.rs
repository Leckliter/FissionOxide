//! The Render workspace's engine: physically based materials and their library, image-based
//! environments, a render camera with depth of field and exposure, and an offline path tracer
//! (CPU, multithreaded, progressive, denoised) that writes PNGs.
//!
//! It knows nothing of designs: `fox-engine` turns a design's bodies, materials and settings
//! into a [`RenderScene`], and the GPU viewport (`fox-render-gl`) previews the same materials
//! and environment in real time from the data here ([`EnvMap`], [`Texture`]).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

pub mod bsdf;
pub mod bvh;
pub mod camera;
pub mod denoise;
pub mod env;
pub mod gallery;
pub mod material;
mod output;
pub mod sampling;
pub mod texture;
pub mod tracer;

pub use camera::{RenderCamera, ToneMap};
pub use env::{Background, EnvMap, Environment, Preset};
pub use gallery::{GalleryImage, Png};
pub use material::{Axis, Material, Pattern, Projection, TextureSource, TextureSpec};
pub use texture::Texture;
pub use tracer::{Progressive, RenderImage, RenderMesh, RenderScene, RenderSettings, render};

#[cfg(test)]
mod tests;
