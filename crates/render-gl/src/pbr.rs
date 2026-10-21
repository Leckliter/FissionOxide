//! Real-time physically based shading for the Render workspace: body meshes drawn with their
//! render materials (metallic/roughness, clear coat, transmission, projected textures), lit by
//! the environment map (reflections from a mipmapped copy, diffuse light from its spherical
//! harmonics), over the environment or a background, on a ground plane catching the key light's
//! shadow. It previews what `fox-raytrace` renders offline, from the same data and the same
//! texture projections.
#![allow(unsafe_code)]
// glow calls are `unsafe`; see `gl.rs` for the contract (a current context, objects from it).

use std::collections::HashMap;

use fox_raytrace::{Background, EnvMap, Environment, Material, RenderCamera, Texture};
use glam::{Mat4, Vec3};
use glow::HasContext;

use crate::gl::{Gl, Prog, program};

/// What the viewport looks like in the Render workspace.
pub struct Look<'a> {
  pub env: &'a EnvMap,
  pub environment: &'a Environment,
  pub camera: &'a RenderCamera,
  /// The material of each mesh of the scene last set (`None`: drawn as usual, e.g. sketches and
  /// selected faces).
  pub materials: &'a [Option<Material>],
  /// Where each mesh's texture projection is centred.
  pub origins: &'a [[f32; 3]],
  /// Bounds of the model (for the ground and the shadow map).
  pub bounds: ([f32; 3], [f32; 3]),
}

/// Largest environment texture width uploaded.
const ENV_WIDTH: u32 = 512;
const SHADOW_SIZE: i32 = 2048;
/// How dark the ground's shadow gets (as in the path tracer).
const SHADOW_STRENGTH: f32 = 0.85;

const PBR_VS: &str = r#"
uniform mat4 u_vp;
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_nrm;
layout(location = 2) in vec4 a_col;
layout(location = 3) in uint a_id;
out vec3 v_nrm;
out vec3 v_wpos;
flat out uint v_id;
void main() {
  v_nrm = a_nrm;
  v_wpos = a_pos;
  v_id = a_id;
  gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

/// Shared by the mesh and background shaders: environment lookups, tone mapping, shadows.
const COMMON: &str = r#"
uniform sampler2D u_env;
uniform float u_env_levels;
uniform float u_env_rot;
uniform vec3 u_sh[9];
uniform float u_exposure;
uniform int u_tone;
uniform sampler2D u_shadow;
uniform mat4 u_light_vp;
uniform int u_shadow_on;
const float PI = 3.14159265;
vec3 env_at(vec3 d, float lod) {
  float phi = atan(d.y, d.x) - u_env_rot;
  float u = fract(phi / (2.0 * PI));
  float v = acos(clamp(d.z, -1.0, 1.0)) / PI;
  return textureLod(u_env, vec2(u, v), lod).rgb;
}
vec3 sh_irr(vec3 n) {
  vec3 c = u_sh[0] * 0.282095
    + u_sh[1] * (0.488603 * n.y) + u_sh[2] * (0.488603 * n.z) + u_sh[3] * (0.488603 * n.x)
    + u_sh[4] * (1.092548 * n.x * n.y) + u_sh[5] * (1.092548 * n.y * n.z)
    + u_sh[6] * (0.315392 * (3.0 * n.z * n.z - 1.0)) + u_sh[7] * (1.092548 * n.x * n.z)
    + u_sh[8] * (0.546274 * (n.x * n.x - n.y * n.y));
  return max(c, vec3(0.0));
}
vec3 tone(vec3 c) {
  c *= u_exposure;
  if (u_tone == 0) {
    c = clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), 0.0, 1.0);
  } else if (u_tone == 1) {
    c = c / (c + 1.0);
  } else {
    c = clamp(c, 0.0, 1.0);
  }
  vec3 lo = c * 12.92;
  vec3 hi = 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055;
  return mix(lo, hi, step(vec3(0.0031308), c));
}
float shadow_at(vec3 p) {
  if (u_shadow_on == 0) return 0.0;
  vec4 c = u_light_vp * vec4(p, 1.0);
  vec3 q = c.xyz / c.w * 0.5 + 0.5;
  if (q.x <= 0.0 || q.x >= 1.0 || q.y <= 0.0 || q.y >= 1.0 || q.z >= 1.0) return 0.0;
  vec2 texel = 1.0 / vec2(textureSize(u_shadow, 0));
  float s = 0.0;
  for (int y = -2; y <= 2; y++) {
    for (int x = -2; x <= 2; x++) {
      float d = texture(u_shadow, q.xy + vec2(float(x), float(y)) * texel * 1.5).r;
      s += q.z - 0.002 > d ? 1.0 : 0.0;
    }
  }
  return s / 25.0;
}
"#;

const PBR_FS: &str = r#"
uniform vec3 u_eye;
uniform vec3 u_head;
uniform int u_persp;
uniform vec3 u_base;
uniform vec4 u_mat;
uniform vec4 u_mat2;
uniform vec3 u_emit;
uniform int u_tex_on;
uniform sampler2D u_tex;
uniform ivec2 u_proj;
uniform vec4 u_tex_xf;
uniform vec3 u_tex_misc;
uniform vec3 u_origin;
uniform vec4 u_hl_color;
uniform uint u_hl;
uniform int u_has_hl;
in vec3 v_nrm;
in vec3 v_wpos;
flat in uint v_id;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
COMMON
vec2 planar(vec3 p, int axis) {
  return axis == 0 ? p.yz : (axis == 1 ? p.xz : p.xy);
}
// mirrors fox_raytrace::texture::project
vec2 proj_uv(vec3 p, vec3 n) {
  float scale = max(u_tex_xf.x, 1e-3);
  vec2 raw;
  if (u_proj.x == 0) {
    vec3 a = abs(n);
    int axis = (a.x >= a.y && a.x >= a.z) ? 0 : (a.y >= a.z ? 1 : 2);
    raw = planar(p, axis) / scale;
  } else if (u_proj.x == 1) {
    raw = planar(p, u_proj.y) / scale;
  } else {
    vec2 q = u_proj.y == 0 ? p.yz : (u_proj.y == 1 ? p.zx : p.xy);
    float h = u_proj.y == 0 ? p.x : (u_proj.y == 1 ? p.y : p.z);
    float u = atan(q.y, q.x) / (2.0 * PI) + 0.5;
    if (u_proj.x == 2) {
      raw = vec2(u, h / scale);
    } else {
      raw = vec2(u, acos(clamp(h / max(length(p), 1e-9), -1.0, 1.0)) / PI);
    }
  }
  float s = u_tex_xf.y, c = u_tex_xf.z;
  return vec2(raw.x * c - raw.y * s, raw.x * s + raw.y * c) + u_tex_misc.xy;
}
vec2 env_brdf(float nv, float r) {
  vec4 c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
  vec4 c1 = vec4(1.0, 0.0425, 1.04, -0.04);
  vec4 r4 = r * c0 + c1;
  float a004 = min(r4.x * r4.x, exp2(-9.28 * nv)) * r4.x + r4.y;
  return vec2(-1.04, 1.04) * a004 + r4.zw;
}
void main() {
  vec3 n = normalize(v_nrm);
  vec3 v = u_persp == 1 ? normalize(u_eye - v_wpos) : u_head;
  if (dot(n, v) < 0.0) n = -n;
  float metallic = u_mat.x, rough = u_mat.y, cc = u_mat.z, cc_rough = u_mat.w;
  float trans = u_mat2.x, ior = u_mat2.y;
  vec3 base = u_base;
  if (u_tex_on == 1) {
    // rows run top-down on the CPU, bottom-up here
    vec2 uv = proj_uv(v_wpos - u_origin, n);
    vec3 t = texture(u_tex, vec2(uv.x, 1.0 - uv.y)).rgb;
    float lum = dot(t, vec3(0.2126, 0.7152, 0.0722));
    rough = clamp(rough + (lum / max(u_mat2.z, 1e-3) - 1.0) * u_tex_misc.z, 0.0, 1.0);
    base = t;
  }
  float nv = max(dot(n, v), 1e-4);
  float f0d = pow((ior - 1.0) / (ior + 1.0), 2.0);
  vec3 f0 = mix(vec3(f0d), base, metallic);
  vec3 r = reflect(-v, n);
  vec2 ab = env_brdf(nv, rough);
  vec3 spec = env_at(r, rough * u_env_levels) * (f0 * ab.x + ab.y);
  float fd = 1.0 - (f0d + (1.0 - f0d) * pow(1.0 - nv, 5.0));
  float shade = 1.0 - 0.6 * shadow_at(v_wpos + n * 0.01);
  vec3 diffuse = sh_irr(n) * base * (1.0 - metallic) * (1.0 - trans) * fd * shade;
  vec3 c = diffuse + spec * mix(1.0, shade, 0.5);
  if (trans > 0.0) {
    // light through the glass: the environment behind it, bent and tinted
    vec3 t = refract(-v, n, 1.0 / ior);
    c += env_at(t, rough * u_env_levels) * base * trans * (1.0 - metallic) * fd;
  }
  if (cc > 0.0) {
    float fc = (0.04 + 0.96 * pow(1.0 - nv, 5.0)) * cc;
    c = c * (1.0 - fc) + env_at(r, cc_rough * u_env_levels) * fc;
  }
  c += u_emit;
  vec3 outc = tone(c);
  if (u_has_hl == 1 && v_id == u_hl) outc = mix(outc, u_hl_color.rgb, 0.45);
  // glass lets what's behind it show through
  float alpha = 1.0 - trans * (1.0 - metallic) * 0.7 * fd;
  o_color = vec4(outc, alpha);
  o_id = id_color(v_id);
}
"#;

/// The background: the environment, a colour or a gradient, and the ground plane under the
/// model as a shadow catcher.
const BG_FS: &str = r#"
uniform mat4 u_inv_vp;
uniform vec2 u_size;
uniform int u_bg;
uniform vec3 u_bg_top;
uniform vec3 u_bg_bottom;
uniform float u_ground_z;
uniform int u_ground;
in vec2 v_uv;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
COMMON
void main() {
  vec2 ndc = v_uv * 2.0 - 1.0;
  vec4 a = u_inv_vp * vec4(ndc, -1.0, 1.0);
  vec4 b = u_inv_vp * vec4(ndc, 1.0, 1.0);
  vec3 o = a.xyz / a.w;
  vec3 d = normalize(b.xyz / b.w - o);
  float t = 1.0 - (gl_FragCoord.y - 0.5) / max(u_size.y - 1.0, 1.0);
  vec3 c = u_bg == 0 ? tone(env_at(d, 0.0)) : mix(u_bg_top, u_bg_bottom, u_bg == 2 ? clamp(t, 0.0, 1.0) : 0.0);
  if (u_ground == 1 && d.z < -1e-6 && o.z > u_ground_z) {
    vec3 p = o + d * ((u_ground_z - o.z) / d.z);
    c *= 1.0 - shadow_at(p) * SHADOW_STRENGTH;
  }
  o_color = vec4(c, 1.0);
  o_id = vec4(0.0);
}
"#;

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

/// A mesh's material, ready for the shader.
struct GpuMaterial {
  base: [f32; 3],
  mat: [f32; 4],
  mat2: [f32; 4],
  emit: [f32; 3],
  texture: Option<(String, [i32; 2], [f32; 4], [f32; 3])>,
  origin: [f32; 3],
  transmissive: bool,
}

/// The PBR passes and their GPU objects.
pub struct Pbr {
  mesh: Prog,
  background: Prog,
  shadow: Prog,
  env_tex: Option<glow::Texture>,
  env_key: String,
  env_levels: f32,
  env_rot: f32,
  sh: [f32; 27],
  key: Vec3,
  textures: HashMap<String, glow::Texture>,
  materials: Vec<Option<GpuMaterial>>,
  shadow_fbo: Option<(glow::Framebuffer, glow::Texture, glow::Texture)>,
  light_vp: Mat4,
  shadow_dirty: bool,
  environment: Environment,
  exposure: f32,
  tone: i32,
  ground_z: f32,
}

fn f16_bits(v: f32) -> u16 {
  let b = v.clamp(0.0, 65000.0).to_bits();
  let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
  let mant = b & 0x7f_ffff;
  if exp <= 0 {
    0
  } else if exp >= 31 {
    0x7bff
  } else {
    ((exp as u32) << 10 | (mant + 0x1000) >> 13).min(0x7bff) as u16
  }
}

fn srgb_lin(c: [u8; 3]) -> [f32; 3] {
  c.map(fox_raytrace::sampling::srgb_to_linear)
}

impl Pbr {
  pub(crate) fn new(gl: &Gl, header: &str) -> Result<Pbr, String> {
    let common = COMMON.to_string();
    let fs = PBR_FS.replace("COMMON", &common).replace("ID_COLOR", crate::gl::ID_COLOR);
    let common_u = [
      "u_env",
      "u_env_levels",
      "u_env_rot",
      "u_sh",
      "u_exposure",
      "u_tone",
      "u_shadow",
      "u_light_vp",
      "u_shadow_on",
    ];
    let mut mesh_u = vec![
      "u_vp",
      "u_eye",
      "u_head",
      "u_persp",
      "u_base",
      "u_mat",
      "u_mat2",
      "u_emit",
      "u_tex_on",
      "u_tex",
      "u_proj",
      "u_tex_xf",
      "u_tex_misc",
      "u_origin",
      "u_hl_color",
      "u_hl",
      "u_has_hl",
    ];
    mesh_u.extend(common_u);
    let mesh = program(gl, header, PBR_VS, &fs, &mesh_u)?;
    let bg_fs = BG_FS.replace("COMMON", &common).replace("SHADOW_STRENGTH", &format!("{SHADOW_STRENGTH:.3}"));
    let mut bg_u = vec!["u_inv_vp", "u_size", "u_bg", "u_bg_top", "u_bg_bottom", "u_ground_z", "u_ground"];
    bg_u.extend(common_u);
    let background = program(gl, header, crate::gl::FULLSCREEN_VS, &bg_fs, &bg_u)?;
    let shadow = program(gl, header, SHADOW_VS, SHADOW_FS, &["u_vp"])?;
    Ok(Pbr {
      mesh,
      background,
      shadow,
      env_tex: None,
      env_key: String::new(),
      env_levels: 1.0,
      env_rot: 0.0,
      sh: [0.0; 27],
      key: Vec3::Z,
      textures: HashMap::new(),
      materials: Vec::new(),
      shadow_fbo: None,
      light_vp: Mat4::IDENTITY,
      shadow_dirty: true,
      environment: Environment::default(),
      exposure: 1.0,
      tone: 0,
      ground_z: 0.0,
    })
  }

  /// Takes a look: uploads what changed (the environment map, textures) and the per-mesh
  /// materials.
  pub(crate) fn set(&mut self, gl: &Gl, look: &Look) -> Result<(), String> {
    let e = look.environment;
    let key = format!("{:?}|{:?}|{}|{}", e.preset, e.hdri, e.intensity, look.env.width);
    if key != self.env_key || self.env_tex.is_none() {
      self.upload_env(gl, look.env)?;
      self.env_key = key;
      let sh = look.env.sh9();
      for (k, c) in sh.iter().enumerate() {
        self.sh[k * 3..k * 3 + 3].copy_from_slice(&c.to_array());
      }
    }
    self.key = look.env.key_direction();
    self.env_rot = look.env.rotation;
    self.environment = e.clone();
    self.exposure = 2f32.powf(look.camera.exposure);
    self.tone = look.camera.tone_map.index();
    let (lo, hi) = (Vec3::from(look.bounds.0), Vec3::from(look.bounds.1));
    self.ground_z = lo.z - e.ground_offset.max(0.0);
    self.light_vp = light_matrix(self.key, lo, hi);
    self.shadow_dirty = true;
    self.materials.clear();
    for (i, m) in look.materials.iter().enumerate() {
      let origin = look.origins.get(i).copied().unwrap_or([0.0; 3]);
      let gm = match m {
        Some(m) => Some(self.material(gl, m, origin)?),
        None => None,
      };
      self.materials.push(gm);
    }
    Ok(())
  }

  fn material(&mut self, gl: &Gl, m: &Material, origin: [f32; 3]) -> Result<GpuMaterial, String> {
    let texture = match &m.texture {
      None => None,
      Some(spec) => {
        let key = format!("{:?}|{:?}|{:?}", spec.source, m.color, spec.color2);
        if !self.textures.contains_key(&key) {
          // an unreadable image leaves the plain colour
          if let Some(Ok(t)) = Texture::for_material(m) {
            let tex = upload_texture(gl, &t)?;
            self.textures.insert(key.clone(), tex);
          }
        }
        let (s, c) = spec.rotation.to_radians().sin_cos();
        self.textures.contains_key(&key).then(|| {
          (
            key,
            [spec.projection.index(), spec.axis.index() as i32],
            [spec.scale, s, c, 0.0],
            [spec.offset[0], spec.offset[1], spec.roughness_variation],
          )
        })
      }
    };
    let base = srgb_lin(m.color);
    let base_lum = 0.2126 * base[0] + 0.7152 * base[1] + 0.0722 * base[2];
    let emit = srgb_lin(m.emission).map(|c| c * m.emission_strength.max(0.0));
    Ok(GpuMaterial {
      base,
      mat: [m.metallic, m.roughness, m.clearcoat, m.clearcoat_roughness].map(|v| v.clamp(0.0, 1.0)),
      mat2: [m.transmission.clamp(0.0, 1.0), m.ior.clamp(1.0, 3.0), base_lum, 0.0],
      emit,
      texture,
      origin,
      transmissive: m.transmission > 0.0 && m.metallic < 1.0,
    })
  }

  fn upload_env(&mut self, gl: &Gl, env: &EnvMap) -> Result<(), String> {
    let (w, h, data) = env.downsampled(ENV_WIDTH);
    unsafe {
      if let Some(t) = self.env_tex.take() {
        gl.delete_texture(t);
      }
      let tex = gl.create_texture()?;
      gl.bind_texture(glow::TEXTURE_2D, Some(tex));
      // mipmaps made here: half-float textures can't be rendered to (so no glGenerateMipmap) on
      // OpenGL ES 3.0
      let (mut lw, mut lh, mut level_data) = (w as usize, h as usize, data);
      let mut level = 0;
      loop {
        let mut px: Vec<u16> = Vec::with_capacity(lw * lh * 4);
        for c in &level_data {
          px.extend_from_slice(&[f16_bits(c.x), f16_bits(c.y), f16_bits(c.z), f16_bits(1.0)]);
        }
        gl.tex_image_2d(
          glow::TEXTURE_2D,
          level,
          glow::RGBA16F as i32,
          lw as i32,
          lh as i32,
          0,
          glow::RGBA,
          glow::HALF_FLOAT,
          glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&px))),
        );
        if lw == 1 && lh == 1 {
          break;
        }
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let mut next = Vec::with_capacity(nw * nh);
        for y in 0..nh {
          for x in 0..nw {
            let at = |i: usize, j: usize| level_data[j.min(lh - 1) * lw + i.min(lw - 1)];
            next.push((at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1)) / 4.0);
          }
        }
        (lw, lh, level_data) = (nw, nh, next);
        level += 1;
      }
      self.env_levels = level as f32;
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
      gl.bind_texture(glow::TEXTURE_2D, None);
      self.env_tex = Some(tex);
    }
    Ok(())
  }

  pub(crate) fn has(&self, mesh: usize) -> bool {
    self.materials.get(mesh).is_some_and(Option::is_some)
  }

  fn bind_common(&self, gl: &Gl, p: &Prog) {
    unsafe {
      gl.active_texture(glow::TEXTURE1);
      gl.bind_texture(glow::TEXTURE_2D, self.env_tex);
      gl.uniform_1_i32(p.loc("u_env"), 1);
      gl.uniform_1_f32(p.loc("u_env_levels"), self.env_levels - 1.0);
      gl.uniform_1_f32(p.loc("u_env_rot"), self.env_rot);
      gl.uniform_3_f32_slice(p.loc("u_sh"), &self.sh);
      gl.uniform_1_f32(p.loc("u_exposure"), self.exposure);
      gl.uniform_1_i32(p.loc("u_tone"), self.tone);
      gl.active_texture(glow::TEXTURE2);
      let shadow = self.shadow_fbo.as_ref().map(|s| s.1);
      gl.bind_texture(glow::TEXTURE_2D, shadow);
      gl.uniform_1_i32(p.loc("u_shadow"), 2);
      gl.uniform_matrix_4_f32_slice(p.loc("u_light_vp"), false, &self.light_vp.to_cols_array());
      gl.uniform_1_i32(p.loc("u_shadow_on"), (self.environment.shadows && shadow.is_some()) as i32);
      gl.active_texture(glow::TEXTURE0);
    }
  }

  /// Renders the shadow map from the key light, when the scene or look changed.
  pub(crate) fn shadow_pass(&mut self, gl: &Gl, meshes: &[(glow::VertexArray, i32, bool)]) -> Result<(), String> {
    if !self.shadow_dirty {
      return Ok(());
    }
    self.shadow_dirty = false;
    unsafe {
      if self.shadow_fbo.is_none() {
        let depth = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(depth));
        gl.tex_image_2d(
          glow::TEXTURE_2D,
          0,
          glow::DEPTH_COMPONENT32F as i32,
          SHADOW_SIZE,
          SHADOW_SIZE,
          0,
          glow::DEPTH_COMPONENT,
          glow::FLOAT,
          glow::PixelUnpackData::Slice(None),
        );
        for (k, v) in [
          (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
          (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
          (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
          (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
          gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
        }
        // a colour attachment too: some drivers want one
        let color = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(color));
        gl.tex_image_2d(
          glow::TEXTURE_2D,
          0,
          glow::R8 as i32,
          SHADOW_SIZE,
          SHADOW_SIZE,
          0,
          glow::RED,
          glow::UNSIGNED_BYTE,
          glow::PixelUnpackData::Slice(None),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depth), 0);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        if status != glow::FRAMEBUFFER_COMPLETE {
          gl.delete_framebuffer(fbo);
          gl.delete_texture(depth);
          gl.delete_texture(color);
          return Err(format!("the shadow framebuffer is incomplete (status {status:#x})"));
        }
        self.shadow_fbo = Some((fbo, depth, color));
      }
      let Some((fbo, ..)) = self.shadow_fbo else { return Ok(()) };
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
      gl.viewport(0, 0, SHADOW_SIZE, SHADOW_SIZE);
      gl.enable(glow::DEPTH_TEST);
      gl.depth_func(glow::LESS);
      gl.depth_mask(true);
      gl.clear_depth_f32(1.0);
      gl.clear(glow::DEPTH_BUFFER_BIT | glow::COLOR_BUFFER_BIT);
      let p = &self.shadow;
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, &self.light_vp.to_cols_array());
      for (vao, count, casts) in meshes {
        if *casts {
          gl.bind_vertex_array(Some(*vao));
          gl.draw_elements(glow::TRIANGLES, *count, glow::UNSIGNED_INT, 0);
        }
      }
      gl.bind_vertex_array(None);
    }
    Ok(())
  }

  /// The background and the ground (depth test off, before the meshes).
  pub(crate) fn draw_background(&self, gl: &Gl, inv_vp: &Mat4, size: [f32; 2], empty_vao: glow::VertexArray) {
    let p = &self.background;
    let e = &self.environment;
    let rgb = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    let bg = match e.background {
      Background::Environment => 0,
      Background::Color | Background::Transparent => 1,
      Background::Gradient => 2,
    };
    unsafe {
      gl.use_program(Some(p.program));
      self.bind_common(gl, p);
      gl.uniform_matrix_4_f32_slice(p.loc("u_inv_vp"), false, &inv_vp.to_cols_array());
      gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
      gl.uniform_1_i32(p.loc("u_bg"), bg);
      gl.uniform_3_f32_slice(p.loc("u_bg_top"), &rgb(e.background_color));
      gl.uniform_3_f32_slice(p.loc("u_bg_bottom"), &rgb(e.background_color2));
      gl.uniform_1_f32(p.loc("u_ground_z"), self.ground_z);
      gl.uniform_1_i32(p.loc("u_ground"), e.ground as i32);
      gl.bind_vertex_array(Some(empty_vao));
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
    }
  }

  /// Draws the meshes that have a material: the opaque ones, or the see-through ones.
  pub(crate) fn draw_meshes(
    &self,
    gl: &Gl,
    meshes: &[(glow::VertexArray, i32, Option<u32>, [u8; 4])],
    view: &MeshView,
    transparent: bool,
  ) {
    let p = &self.mesh;
    unsafe {
      gl.use_program(Some(p.program));
      self.bind_common(gl, p);
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, &view.vp);
      gl.uniform_3_f32_slice(p.loc("u_eye"), &view.eye);
      gl.uniform_3_f32_slice(p.loc("u_head"), &view.head);
      gl.uniform_1_i32(p.loc("u_persp"), view.persp as i32);
      for (i, (vao, count, hl, hl_color)) in meshes.iter().enumerate() {
        let Some(Some(m)) = self.materials.get(i) else { continue };
        if m.transmissive != transparent {
          continue;
        }
        gl.uniform_3_f32_slice(p.loc("u_base"), &m.base);
        gl.uniform_4_f32_slice(p.loc("u_mat"), &m.mat);
        gl.uniform_4_f32_slice(p.loc("u_mat2"), &m.mat2);
        gl.uniform_3_f32_slice(p.loc("u_emit"), &m.emit);
        gl.uniform_3_f32_slice(p.loc("u_origin"), &m.origin);
        match &m.texture {
          Some((key, proj, xf, misc)) => {
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, self.textures.get(key).copied());
            gl.uniform_1_i32(p.loc("u_tex"), 3);
            gl.uniform_1_i32(p.loc("u_tex_on"), 1);
            gl.uniform_2_i32(p.loc("u_proj"), proj[0], proj[1]);
            gl.uniform_4_f32_slice(p.loc("u_tex_xf"), xf);
            gl.uniform_3_f32_slice(p.loc("u_tex_misc"), misc);
            gl.active_texture(glow::TEXTURE0);
          }
          None => gl.uniform_1_i32(p.loc("u_tex_on"), 0),
        }
        gl.uniform_4_f32_slice(p.loc("u_hl_color"), &hl_color.map(|c| c as f32 / 255.0));
        gl.uniform_1_u32(p.loc("u_hl"), hl.unwrap_or(0));
        gl.uniform_1_i32(p.loc("u_has_hl"), hl.is_some() as i32);
        gl.bind_vertex_array(Some(*vao));
        gl.draw_elements(glow::TRIANGLES, *count, glow::UNSIGNED_INT, 0);
      }
    }
  }

  pub(crate) fn destroy(self, gl: &Gl) {
    unsafe {
      for p in [&self.mesh, &self.background, &self.shadow] {
        gl.delete_program(p.program);
      }
      if let Some(t) = self.env_tex {
        gl.delete_texture(t);
      }
      for t in self.textures.into_values() {
        gl.delete_texture(t);
      }
      if let Some((f, d, c)) = self.shadow_fbo {
        gl.delete_framebuffer(f);
        gl.delete_texture(d);
        gl.delete_texture(c);
      }
    }
  }
}

/// Camera data the mesh pass needs.
pub(crate) struct MeshView {
  pub vp: [f32; 16],
  pub eye: [f32; 3],
  pub head: [f32; 3],
  pub persp: bool,
}

/// An orthographic projection looking along `-key` that holds the model's box.
fn light_matrix(key: Vec3, lo: Vec3, hi: Vec3) -> Mat4 {
  let key = if key.length_squared() < 1e-6 { Vec3::Z } else { key.normalize() };
  let c = (lo + hi) / 2.0;
  let r = ((hi - lo).length() / 2.0).max(1.0);
  let up = if key.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
  let view = Mat4::look_at_rh(c + key * r * 2.0, c, up);
  // the ground around the model too, where its shadow falls
  let reach = r * 2.5;
  Mat4::orthographic_rh_gl(-reach, reach, -reach, reach, r * 0.5, r * 4.0) * view
}

fn upload_texture(gl: &Gl, t: &Texture) -> Result<glow::Texture, String> {
  let px = t.to_srgb8();
  unsafe {
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
    gl.tex_image_2d(
      glow::TEXTURE_2D,
      0,
      glow::SRGB8_ALPHA8 as i32,
      t.width as i32,
      t.height as i32,
      0,
      glow::RGBA,
      glow::UNSIGNED_BYTE,
      glow::PixelUnpackData::Slice(Some(&px)),
    );
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(tex)
  }
}
