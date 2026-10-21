//! An OpenGL renderer for [`fox_render::Scene`]s: the desktop viewport's GPU path.
//!
//! It draws the same scene description as the software rasterizer in `fox-render` (shaded
//! meshes with appearances and surface analysis, depth-tested and on-top polylines, markers,
//! the background gradient) with the same lighting, line widths and depth biases, so the two
//! paths look alike and either can stand in for the other. The CPU rasterizer stays the
//! renderer of the CLI and MCP server, and the fallback when no OpenGL 3.3 / ES 3.0 context is
//! available.
//!
//! The scene's geometry lives in GPU buffers, so moving the camera costs one draw of what's
//! already there instead of a re-rasterization on the CPU. Picking stays exact: every fragment
//! also writes its pick id into a second colour attachment (the same ids the CPU path writes
//! into [`fox_render::Frame::ids`]), which [`Renderer::read_ids`] reads back.
//!
//! All OpenGL calls take the caller's `glow::Context`, which must be current.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod gl;
#[cfg(feature = "headless")]
pub mod headless;
mod pack;
pub mod pbr;

pub use gl::Renderer;
pub use glow;

/// Ids at or above this don't fit the id attachment (24 bits) and read back as `id & MAX_ID - 1`.
pub const MAX_ID: u32 = 1 << 24;
