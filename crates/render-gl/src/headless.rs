//! A headless OpenGL context (EGL on a device, surfaceless) for tests and benchmarks: Mesa's
//! software driver provides one on machines without a GPU. Behind the `headless` feature.
#![allow(unsafe_code)]

use glutin::api::egl::context::PossiblyCurrentContext;
use glutin::api::egl::{device::Device, display::Display};
use glutin::config::{ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, Version};
use glutin::prelude::*;

/// A current context with nothing to draw on but offscreen framebuffers.
pub struct Headless {
  pub gl: glow::Context,
  _ctx: PossiblyCurrentContext,
  _display: Display,
}

/// A current OpenGL 3.3 core context, or why there is none.
pub fn context() -> Result<Headless, String> {
  let devices = Device::query_devices().map_err(|e| e.to_string())?;
  let mut last = "no EGL devices".to_string();
  for device in devices {
    match try_device(&device) {
      Ok(h) => return Ok(h),
      Err(e) => last = e,
    }
  }
  Err(last)
}

fn try_device(device: &Device) -> Result<Headless, String> {
  // SAFETY: no raw display handle is passed.
  let display = unsafe { Display::with_device(device, None) }.map_err(|e| e.to_string())?;
  let template = ConfigTemplateBuilder::new().with_surface_type(ConfigSurfaceTypes::empty()).build();
  // SAFETY: the template is valid; the display outlives the configs.
  let config = unsafe { display.find_configs(template) }.map_err(|e| e.to_string())?.next().ok_or("no EGL config")?;
  let attrs = ContextAttributesBuilder::new()
    .with_profile(GlProfile::Core)
    .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
    .build(None);
  // SAFETY: the config came from this display.
  let ctx = unsafe { display.create_context(&config, &attrs) }.map_err(|e| e.to_string())?;
  let ctx = ctx.make_current_surfaceless().map_err(|e| e.to_string())?;
  // SAFETY: the context is current on this thread and stays alive in `Headless`.
  let gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s).cast()) };
  Ok(Headless { gl, _ctx: ctx, _display: display })
}

impl Headless {
  /// Waits for the GPU to finish what it was given (for timing).
  pub fn finish(&self) {
    // SAFETY: the context is current (see `context`).
    unsafe { glow::HasContext::finish(&self.gl) }
  }

  /// Leaves the state egui's painter leaves for a paint callback, which the viewport draws in:
  /// the scissor test on, clipped to `rect` (x, y, width, height in pixels).
  pub fn scissor_like_egui(&self, rect: [i32; 4]) {
    // SAFETY: as above.
    unsafe {
      glow::HasContext::enable(&self.gl, glow::SCISSOR_TEST);
      glow::HasContext::scissor(&self.gl, rect[0], rect[1], rect[2], rect[3]);
    }
  }

  /// The driver's name for the renderer, e.g. `llvmpipe (LLVM 20.1.2, 256 bits)`.
  pub fn renderer_name(&self) -> String {
    // SAFETY: as above.
    unsafe { glow::HasContext::get_parameter_string(&self.gl, glow::RENDERER) }
  }
}
