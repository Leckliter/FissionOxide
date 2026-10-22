//! The effects drawn into the picture (`fox_render::Post`) on the GPU: Object Shadow's shadow map
//! (sampled by the mesh shader), and the Ambient Occlusion and Anti-Aliasing passes over the
//! offscreen target, doing the CPU rasterizer's arithmetic (`fox_render::post`) per fragment.
//! Environment Dome is the background shader's (see `gl.rs`).
#![allow(unsafe_code)]
// glow calls are `unsafe`; see `gl.rs` for the contract (a current context, objects from it).

use glow::HasContext;

use crate::gl::{FULLSCREEN_VS, Gl, Prog, program};

const SHADOW_VS: &str = r#"
uniform mat4 u_vp;
layout(location = 0) in vec3 a_pos;
void main() {
  gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

const SHADOW_FS: &str = r#"
layout(location = 0) out vec4 o_color;
void main() {
  o_color = vec4(1.0);
}
"#;

/// Object Shadow in the mesh shader: `fox_render::ShadowMap::shade`.
pub(crate) const SHADOW_GLSL: &str = r#"
uniform int u_obj_shadow;
uniform sampler2D u_shadow_map;
uniform mat4 u_light_vp;
uniform vec3 u_to_light;
uniform float u_shadow_texel;
float obj_occlusion(vec3 p) {
  vec4 c = u_light_vp * vec4(p, 1.0);
  vec3 q = c.xyz / c.w * 0.5 + 0.5;
  if (q.x <= 0.0 || q.x >= 1.0 || q.y <= 0.0 || q.y >= 1.0 || q.z >= 1.0) return 0.0;
  int n = textureSize(u_shadow_map, 0).x;
  ivec2 at = ivec2(floor(q.xy * float(n)));
  float hit = 0.0;
  for (int dy = -1; dy <= 1; dy++) {
    for (int dx = -1; dx <= 1; dx++) {
      ivec2 t = clamp(at + ivec2(dx, dy), ivec2(0), ivec2(n - 1));
      if (q.z - SHADOW_BIAS > texelFetch(u_shadow_map, t, 0).r) hit += 1.0;
    }
  }
  return hit / 9.0;
}
vec3 obj_shadow(vec3 c, vec3 p, vec3 n) {
  if (u_obj_shadow == 0 || dot(n, u_to_light) <= SHADOW_MIN_COS) return c;
  return c * (1.0 - SHADOW_DARKEN * obj_occlusion(p + n * (u_shadow_texel * 1.5)));
}
"#;

/// Ambient Occlusion over the surfaces drawn so far: `fox_render::post::ambient_occlusion`.
const AO_FS: &str = r#"
uniform sampler2D u_color;
uniform sampler2D u_depth;
uniform mat4 u_inv;
uniform vec3 u_eye;
uniform vec3 u_fwd;
uniform float u_per_px;
uniform int u_ortho;
uniform float u_dist;
layout(location = 0) out vec4 o_color;
// the linear depth at texel `p`, or -1 where nothing is drawn (or off the picture)
float lin(ivec2 p) {
  ivec2 size = textureSize(u_depth, 0);
  if (p.x < 0 || p.y < 0 || p.x >= size.x || p.y >= size.y) return -1.0;
  float d = texelFetch(u_depth, p, 0).r;
  if (d >= 1.0) return -1.0;
  vec4 ndc = vec4((vec2(p) + 0.5) / vec2(size) * 2.0 - 1.0, d * 2.0 - 1.0, 1.0);
  vec4 w = u_inv * ndc;
  return dot(w.xyz / w.w - u_eye, u_fwd);
}
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  vec4 col = texelFetch(u_color, p, 0);
  float d = lin(p);
  o_color = col;
  if (d < 0.0) return;
  float pxw = u_ortho == 1 ? u_per_px * u_dist : u_per_px * d;
  float occ = 0.0;
  float pairs = 0.0;
  for (int i = 0; i < 2; i++) {
    int r = i == 0 ? AO_R0 : AO_R1;
    int k = int(floor(float(r) * 0.70710677 + 0.5));
    for (int j = 0; j < 4; j++) {
      ivec2 o = j == 0 ? ivec2(r, 0) : (j == 1 ? ivec2(0, r) : (j == 2 ? ivec2(k, k) : ivec2(k, -k)));
      pairs += 1.0;
      float a = lin(p + o);
      float b = lin(p - o);
      if (a < 0.0 || b < 0.0) continue;
      float scale = float(r) * pxw;
      float diff = d - 0.5 * (a + b);
      if (diff > 0.02 * scale && diff < 4.0 * scale) occ += min(diff / scale, 1.0);
    }
  }
  float ao = 1.0 - AO_STRENGTH * occ / pairs;
  o_color = vec4(floor(col.rgb * 255.0 * ao + 0.5) / 255.0, col.a);
}
"#;

/// Anti-Aliasing over the finished picture: `fox_render::post::anti_alias`.
const AA_FS: &str = r#"
uniform sampler2D u_color;
layout(location = 0) out vec4 o_color;
float luma(vec3 c) {
  return dot(c * 255.0, vec3(0.299, 0.587, 0.114));
}
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  ivec2 size = textureSize(u_color, 0);
  vec4 c = texelFetch(u_color, p, 0);
  o_color = c;
  if (p.x < 1 || p.y < 1 || p.x >= size.x - 1 || p.y >= size.y - 1) return;
  vec3 n = texelFetch(u_color, p + ivec2(0, 1), 0).rgb;
  vec3 s = texelFetch(u_color, p - ivec2(0, 1), 0).rgb;
  vec3 e = texelFetch(u_color, p + ivec2(1, 0), 0).rgb;
  vec3 w = texelFetch(u_color, p - ivec2(1, 0), 0).rgb;
  float lc = luma(c.rgb);
  float lo = min(lc, min(min(luma(n), luma(s)), min(luma(e), luma(w))));
  float hi = max(lc, max(max(luma(n), luma(s)), max(luma(e), luma(w))));
  if (hi - lo <= AA_EDGE) return;
  o_color = vec4(floor((0.5 * c.rgb + 0.125 * (n + s + e + w)) * 255.0 + 0.5) / 255.0, c.a);
}
"#;

/// The constants the shaders share with `fox_render::post`.
pub(crate) fn defines() -> String {
  let f = |v: f32| format!("{v:?}");
  format!(
    "#define SHADOW_BIAS {}\n#define SHADOW_MIN_COS {}\n#define SHADOW_DARKEN {}\n#define AO_STRENGTH {}\n#define AO_R0 {}\n#define AO_R1 {}\n#define AA_EDGE {}\n",
    f(fox_render::SHADOW_BIAS),
    f(fox_render::SHADOW_MIN_COS),
    f(fox_render::SHADOW_DARKEN),
    f(fox_render::AO_STRENGTH),
    fox_render::AO_RADII[0],
    fox_render::AO_RADII[1],
    f(fox_render::AA_EDGE),
  )
}

/// A colour and depth copy of the target, for the passes that read the picture they write.
struct Copy {
  fbo: glow::Framebuffer,
  color: glow::Texture,
  depth: glow::Texture,
  size: (u32, u32),
}

/// The effects' GPU objects, made on first use.
pub(crate) struct Fx {
  shadow_prog: Prog,
  ao: Prog,
  aa: Prog,
  /// The shadow map: framebuffer, depth texture, colour texture.
  shadow: Option<(glow::Framebuffer, glow::Texture, glow::Texture)>,
  copy: Option<Copy>,
}

fn texture(gl: &Gl, w: i32, h: i32, internal: u32, format: u32, ty: u32) -> Result<glow::Texture, String> {
  unsafe {
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, w, h, 0, format, ty, glow::PixelUnpackData::Slice(None));
    for (k, v) in [
      (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
      (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
      (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
      (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
    ] {
      gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
    }
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(t)
  }
}

fn framebuffer(gl: &Gl, color: glow::Texture, depth: glow::Texture) -> Result<glow::Framebuffer, String> {
  unsafe {
    let fbo = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depth), 0);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    if status != glow::FRAMEBUFFER_COMPLETE {
      gl.delete_framebuffer(fbo);
      return Err(format!("an effects framebuffer is incomplete (status {status:#x})"));
    }
    Ok(fbo)
  }
}

impl Fx {
  pub(crate) fn new(gl: &Gl, header: &str) -> Result<Fx, String> {
    let header = format!("{header}{}", defines());
    let shadow_prog = program(gl, &header, SHADOW_VS, SHADOW_FS, &["u_vp"])?;
    let ao = program(
      gl,
      &header,
      FULLSCREEN_VS,
      AO_FS,
      &["u_color", "u_depth", "u_inv", "u_eye", "u_fwd", "u_per_px", "u_ortho", "u_dist"],
    )?;
    let aa = program(gl, &header, FULLSCREEN_VS, AA_FS, &["u_color"])?;
    Ok(Fx { shadow_prog, ao, aa, shadow: None, copy: None })
  }

  /// Draws the shadow casters' depth from the light `light_vp`: `meshes` are each mesh's vertex
  /// array and index runs, with whether it casts.
  pub(crate) fn shadow_pass(
    &mut self,
    gl: &Gl,
    light_vp: &glam::Mat4,
    meshes: &[(glow::VertexArray, Vec<std::ops::Range<usize>>, bool)],
  ) -> Result<(), String> {
    let n = fox_render::SHADOW_SIZE as i32;
    if self.shadow.is_none() {
      let depth = texture(gl, n, n, glow::DEPTH_COMPONENT32F, glow::DEPTH_COMPONENT, glow::FLOAT)?;
      let color = texture(gl, n, n, glow::R8, glow::RED, glow::UNSIGNED_BYTE)?;
      let fbo = framebuffer(gl, color, depth).inspect_err(|_| unsafe {
        gl.delete_texture(depth);
        gl.delete_texture(color);
      })?;
      self.shadow = Some((fbo, depth, color));
    }
    let Some((fbo, ..)) = self.shadow else { return Ok(()) };
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
      gl.viewport(0, 0, n, n);
      gl.disable(glow::SCISSOR_TEST);
      gl.disable(glow::BLEND);
      gl.disable(glow::CULL_FACE);
      gl.enable(glow::DEPTH_TEST);
      gl.depth_func(glow::LESS);
      gl.depth_mask(true);
      gl.color_mask(true, true, true, true);
      gl.clear_depth_f32(1.0);
      gl.clear(glow::DEPTH_BUFFER_BIT | glow::COLOR_BUFFER_BIT);
      let p = &self.shadow_prog;
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, &light_vp.to_cols_array());
      for (vao, runs, casts) in meshes {
        if *casts {
          gl.bind_vertex_array(Some(*vao));
          for r in runs {
            gl.draw_elements(glow::TRIANGLES, r.len() as i32, glow::UNSIGNED_INT, (r.start * 4) as i32);
          }
        }
      }
      gl.bind_vertex_array(None);
      gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }
    Ok(())
  }

  /// The shadow map's depth texture, once drawn.
  pub(crate) fn shadow_map(&self) -> Option<glow::Texture> {
    self.shadow.map(|(_, depth, _)| depth)
  }

  /// Copies the colour (and depth) of framebuffer `from`, `size` big, for a pass to read.
  fn copy(&mut self, gl: &Gl, from: glow::Framebuffer, size: (u32, u32)) -> Result<&Copy, String> {
    if self.copy.as_ref().is_some_and(|c| c.size != size) {
      self.free_copy(gl);
    }
    let (w, h) = (size.0 as i32, size.1 as i32);
    if self.copy.is_none() {
      let color = texture(gl, w, h, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE)?;
      let depth = texture(gl, w, h, glow::DEPTH_COMPONENT32F, glow::DEPTH_COMPONENT, glow::FLOAT)?;
      let fbo = framebuffer(gl, color, depth).inspect_err(|_| unsafe {
        gl.delete_texture(depth);
        gl.delete_texture(color);
      })?;
      self.copy = Some(Copy { fbo, color, depth, size });
    }
    let c = self.copy.as_ref().ok_or("no copy")?;
    unsafe {
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(from));
      gl.read_buffer(glow::COLOR_ATTACHMENT0);
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(c.fbo));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
      gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT, glow::NEAREST);
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
    }
    Ok(c)
  }

  /// Runs a fullscreen pass of `prog` into the colour attachment of `target` (its ids and depth
  /// left alone), reading a copy of it; `uniforms` sets the pass's own.
  fn pass(
    &mut self,
    gl: &Gl,
    target: glow::Framebuffer,
    size: (u32, u32),
    empty_vao: glow::VertexArray,
    which: Pass,
    uniforms: &dyn Fn(&Gl, &Prog),
  ) -> Result<(), String> {
    let (color, depth) = {
      let c = self.copy(gl, target, size)?;
      (c.color, c.depth)
    };
    let p = match which {
      Pass::Ao => &self.ao,
      Pass::Aa => &self.aa,
    };
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
      gl.viewport(0, 0, size.0 as i32, size.1 as i32);
      gl.disable(glow::DEPTH_TEST);
      gl.depth_mask(false);
      gl.disable(glow::BLEND);
      gl.use_program(Some(p.program));
      gl.active_texture(glow::TEXTURE0);
      gl.bind_texture(glow::TEXTURE_2D, Some(color));
      gl.uniform_1_i32(p.loc("u_color"), 0);
      gl.active_texture(glow::TEXTURE1);
      gl.bind_texture(glow::TEXTURE_2D, Some(depth));
      gl.uniform_1_i32(p.loc("u_depth"), 1);
      uniforms(gl, p);
      gl.bind_vertex_array(Some(empty_vao));
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
      gl.bind_texture(glow::TEXTURE_2D, None);
      gl.active_texture(glow::TEXTURE0);
      gl.bind_texture(glow::TEXTURE_2D, None);
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
      gl.depth_mask(true);
      gl.enable(glow::DEPTH_TEST);
    }
    Ok(())
  }

  /// Ambient Occlusion over what `target` holds, seen from `cam`.
  pub(crate) fn ambient_occlusion(
    &mut self,
    gl: &Gl,
    target: glow::Framebuffer,
    size: (u32, u32),
    empty_vao: glow::VertexArray,
    cam: &fox_render::Camera,
  ) -> Result<(), String> {
    let inv = cam.view_proj(size.0, size.1).inverse().as_mat4().to_cols_array();
    let eye = cam.eye().as_vec3().to_array();
    let fwd = (-cam.basis().2).as_vec3().to_array();
    let per_px = (cam.view_height() / cam.distance / size.1.max(1) as f64) as f32;
    let set = |gl: &Gl, p: &Prog| unsafe {
      gl.uniform_matrix_4_f32_slice(p.loc("u_inv"), false, &inv);
      gl.uniform_3_f32_slice(p.loc("u_eye"), &eye);
      gl.uniform_3_f32_slice(p.loc("u_fwd"), &fwd);
      gl.uniform_1_f32(p.loc("u_per_px"), per_px);
      gl.uniform_1_i32(p.loc("u_ortho"), cam.ortho as i32);
      gl.uniform_1_f32(p.loc("u_dist"), cam.distance as f32);
    };
    self.pass(gl, target, size, empty_vao, Pass::Ao, &set)
  }

  /// Anti-Aliasing over what `target` holds.
  pub(crate) fn anti_alias(
    &mut self,
    gl: &Gl,
    target: glow::Framebuffer,
    size: (u32, u32),
    empty_vao: glow::VertexArray,
  ) -> Result<(), String> {
    self.pass(gl, target, size, empty_vao, Pass::Aa, &|_, _| {})
  }

  fn free_copy(&mut self, gl: &Gl) {
    if let Some(c) = self.copy.take() {
      unsafe {
        gl.delete_framebuffer(c.fbo);
        gl.delete_texture(c.color);
        gl.delete_texture(c.depth);
      }
    }
  }

  pub(crate) fn destroy(mut self, gl: &Gl) {
    self.free_copy(gl);
    unsafe {
      if let Some((fbo, depth, color)) = self.shadow.take() {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(depth);
        gl.delete_texture(color);
      }
      for p in [&self.shadow_prog, &self.ao, &self.aa] {
        gl.delete_program(p.program);
      }
    }
  }
}

#[derive(Clone, Copy)]
enum Pass {
  Ao,
  Aa,
}
