//! The OpenGL side: shaders, buffers, the offscreen target and the draw passes.
#![allow(unsafe_code)]
// Every function here calls glow, whose calls are `unsafe` because they go straight to the
// driver. They are sound as long as the context passed in is current and the objects used came
// from it, which is the documented contract of [`Renderer`].

use std::ops::Range;

use fox_render::{Analysis, Camera, Frame, NO_ID, Scene, VisualStyle};
use glam::Vec3;
use glow::HasContext;

use crate::pack::{self, Bounds, Cone, FOLD_STRIDE, LINE_STRIDE, MARKER_STRIDE, MESH_STRIDE, MeshChunk};

/// A level of detail is drawn while navigating when its cells look at most this many pixels wide.
pub const LOD_PIXELS: f32 = 1.5;
/// How far (pixels) a line or marker can reach outside its points: chunks this close to the view
/// still draw.
const REACH_PX: f32 = 16.0;

const MESH_VS: &str = r#"
uniform mat4 u_vp;
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_nrm;
layout(location = 2) in vec4 a_col;
layout(location = 3) in uint a_id;
out vec3 v_nrm;
out vec3 v_wpos;
out vec3 v_col;
flat out uint v_id;
void main() {
  v_nrm = a_nrm;
  v_wpos = a_pos;
  v_col = a_col.rgb;
  v_id = a_id;
  gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

/// Texture coordinates of a projected texture: `proj_uv(point - origin, normal)`. Mirrors
/// `fox_raytrace::texture::project` and `fox_render::TexProjection::uv`.
pub(crate) const PROJ_UV: &str = r#"
uniform ivec2 u_proj;
uniform vec4 u_tex_xf;
uniform vec3 u_tex_misc;
vec2 planar(vec3 p, int axis) {
  return axis == 0 ? p.yz : (axis == 1 ? p.xz : p.xy);
}
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
    float u = atan(q.y, q.x) / 6.283185307 + 0.5;
    if (u_proj.x == 2) {
      raw = vec2(u, h / scale);
    } else {
      raw = vec2(u, acos(clamp(h / max(length(p), 1e-9), -1.0, 1.0)) / 3.141592654);
    }
  }
  float s = u_tex_xf.y, c = u_tex_xf.z;
  return vec2(raw.x * c - raw.y * s, raw.x * s + raw.y * c) + u_tex_misc.xy;
}
"#;

/// The CPU rasterizer's `raster_tri` shading, per fragment.
const MESH_FS: &str = r#"
uniform vec3 u_key;
uniform vec3 u_head;
uniform vec3 u_fill;
uniform vec3 u_eye;
uniform int u_persp;
uniform vec4 u_color;
uniform vec4 u_hl_color;
uniform uint u_hl;
uniform int u_has_hl;
uniform int u_vcolor;
uniform int u_mode;
uniform vec4 u_param;
uniform vec4 u_param2;
uniform float u_fade;
uniform int u_tex_on;
uniform sampler2D u_tex;
uniform vec3 u_origin;
in vec3 v_nrm;
in vec3 v_wpos;
in vec3 v_col;
flat in uint v_id;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
OBJ_SHADOW
PROJ_UV
vec3 safe_norm(vec3 v) {
  float l = length(v);
  return l > 0.0 ? v / l : vec3(0.0);
}
void main() {
  bool hl = u_has_hl == 1 && v_id == u_hl;
  vec4 flat_c = hl ? u_hl_color : u_color;
  vec3 base = (u_vcolor == 1 && !hl) ? v_col : flat_c.rgb;
  vec3 n = safe_norm(v_nrm);
  vec3 view = u_persp == 1 ? safe_norm(u_eye - v_wpos) : u_head;
  if (dot(n, view) < 0.0) n = -n;
  if (u_tex_on == 1 && !hl) {
    // rows run top-down on the CPU, bottom-up here
    vec2 uv = proj_uv(v_wpos - u_origin, n);
    base = texture(u_tex, vec2(uv.x, 1.0 - uv.y)).rgb;
  }
  // the CPU renderer's soft shading (see fox_render): plenty of ambient, a gentle key light
  float diff = 0.36 * max(dot(n, u_key), 0.0) + 0.24 * max(dot(n, u_head), 0.0) + 0.15 * max(dot(n, u_fill), 0.0);
  vec3 h = safe_norm(u_key + view);
  float spec = pow(max(dot(n, h), 0.0), 40.0) * 0.12;
  vec3 c;
  if (u_mode == 4 || u_mode == 5) {
    c = base;
  } else if (u_mode == 0) {
    c = base * (0.5 + diff) + vec3(spec);
  } else {
    // fox_render::analysis_tone
    vec3 green = vec3(0.2, 0.75, 0.25);
    vec3 red = vec3(0.85, 0.2, 0.2);
    vec3 yellow = vec3(0.95, 0.85, 0.2);
    vec3 t;
    if (u_mode == 1) {
      vec3 r = n * (2.0 * dot(n, view)) - view;
      float s = u_param.y > 0.5 ? r.x : r.z;
      float band = floor((s * 0.5 + 0.5) * u_param.x);
      float tone = mod(band, 2.0) < 0.5 ? 0.1 : 0.95;
      t = vec3(tone * (0.75 + 0.25 * diff));
    } else if (u_mode == 2) {
      float ang = asin(clamp(dot(n, u_param.xyz), -1.0, 1.0));
      float lo = u_param2.x;
      float hi = u_param2.y;
      float tol = u_param2.z;
      vec3 tone;
      if (ang >= hi) {
        tone = green;
      } else if (ang <= lo) {
        tone = red;
      } else if (tol > 0.0 && abs(ang) <= tol) {
        tone = yellow;
      } else if (ang > 0.0) {
        tone = mix(yellow, green, ang / hi);
      } else {
        tone = mix(yellow, red, ang / lo);
      }
      t = tone * (0.45 + diff);
    } else if (u_mode == 3) {
      float d = dot(n, u_param.xyz);
      t = (d > 1e-3 ? green : red) * (0.45 + diff);
    } else {
      vec3 r = n * (2.0 * dot(n, view)) - view;
      float sn = sin(u_param.x);
      float cs = cos(u_param.x);
      float x = r.x * cs - r.y * sn;
      float y = r.x * sn + r.y * cs;
      float z = u_param.y > 0.5 ? abs(r.z) : r.z;
      float tone = z >= 0.0 ? 0.55 + 0.4 * z : 0.22 + 0.12 * z;
      if (u_param.z > 0.5 && z > 0.2 && z < 0.45 && cos(3.0 * atan(y, x)) > 0.5) tone = 1.0;
      t = vec3(tone * (0.85 + 0.15 * diff));
    }
    c = t + (base * (0.5 + diff) + vec3(spec) - t) * u_fade;
  }
  if (u_mode != 4 && u_mode != 5) c = obj_shadow(c, v_wpos, n);
  o_color = vec4(clamp(c, 0.0, 1.0), flat_c.a);
  o_id = id_color(v_id);
}
"#;

/// Segments as screen-space quads: the CPU brush is a square of `width` pixels swept along the
/// segment, so the quad is that wide and reaches half a width past each end.
const LINE_QUAD: &str = r#"
uniform mat4 u_vp;
uniform vec2 u_size;
uniform int u_hidden;
layout(location = 0) in vec2 a_corner;
out vec4 v_color;
flat out uint v_id;
noperspective out float v_along;
flat out float v_dashed;
// `phase`: how far along its polyline the segment starts, in world units; the dash pattern goes
// on from there at this segment's own scale, as the CPU's runs on along the whole polyline
void line_quad(vec3 a, vec3 b, float width, float bias, float phase) {
  v_along = 0.0;
  vec4 ca = u_vp * vec4(a, 1.0);
  vec4 cb = u_vp * vec4(b, 1.0);
  float da = ca.z + ca.w;
  float db = cb.z + cb.w;
  if (da <= 0.0 && db <= 0.0) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  if (da <= 0.0) {
    ca = ca + (cb - ca) * (da / (da - db));
  } else if (db <= 0.0) {
    cb = ca + (cb - ca) * (da / (da - db));
  }
  vec3 na = ca.xyz / ca.w;
  vec3 nb = cb.xyz / cb.w;
  vec2 sa = (na.xy * 0.5 + 0.5) * u_size;
  vec2 sb = (nb.xy * 0.5 + 0.5) * u_size;
  vec2 d = sb - sa;
  float len = length(d);
  if (!(len <= 1e6)) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec2 dir = len > 1e-6 ? d / len : vec2(1.0, 0.0);
  vec2 nrm = vec2(-dir.y, dir.x);
  // hidden edges are a pixel wide
  float hw = (u_hidden == 1 ? 1.0 : width) * 0.5;
  bool end = a_corner.x > 0.5;
  vec2 s = (end ? sb : sa) + nrm * (a_corner.y * hw) + dir * (end ? hw : -hw);
  float world = distance(a, b);
  v_along = dot(s - sa, dir) + (world > 0.0 ? phase * len / world : 0.0);
  float z = clamp((end ? nb.z : na.z) + bias, -1.0, 1.0);
  gl_Position = vec4(s / u_size * 2.0 - 1.0, z, 1.0);
}
"#;

const LINE_VS: &str = r#"
LINE_QUAD
layout(location = 1) in vec3 a_a;
layout(location = 2) in vec3 a_b;
layout(location = 3) in vec4 a_color;
layout(location = 4) in uint a_id;
layout(location = 5) in vec4 a_info;
void main() {
  v_color = a_color;
  v_id = a_id;
  v_dashed = a_info.z;
  line_quad(a_a, a_b, a_info.x, a_info.y, a_info.w);
}
"#;

/// Fold edges, drawn as one-pixel lines where exactly one of their two triangles faces the eye
/// (`FoldEdge::silhouette`), with the pick id of that one; collapsed elsewhere.
const SILHOUETTE_VS: &str = r#"
LINE_QUAD
uniform int u_persp;
uniform vec3 u_eye;
uniform vec3 u_toward;
uniform vec4 u_color;
// world units per pixel: at distance 1 in perspective, everywhere in orthographic
uniform float u_world_px;
uniform float u_nudge;
// the line's width (pixels), and the cosine below which an edge is drawn wherever the eye is
// (an outline's creases; -2 for none)
uniform float u_width;
uniform float u_crease;
layout(location = 1) in vec3 a_a;
layout(location = 2) in vec3 a_b;
layout(location = 3) in vec3 a_n0;
layout(location = 4) in vec3 a_n1;
layout(location = 5) in uvec2 a_ids;
void main() {
  vec3 to_eye = u_persp == 1 ? u_eye - a_a : u_toward;
  bool f0 = dot(a_n0, to_eye) > 0.0;
  bool f1 = dot(a_n1, to_eye) > 0.0;
  v_color = u_color;
  v_id = f0 ? a_ids.x : a_ids.y;
  v_dashed = 0.0;
  v_along = 0.0;
  if (f0 == f1 && dot(a_n0, a_n1) >= u_crease) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  // just outside the surface it outlines (see SILHOUETTE_NUDGE_PX)
  vec3 n = normalize(a_n0 + a_n1);
  float ka = u_world_px * u_nudge * (u_persp == 1 ? distance(u_eye, a_a) : 1.0);
  float kb = u_world_px * u_nudge * (u_persp == 1 ? distance(u_eye, a_b) : 1.0);
  line_quad(a_a + n * ka, a_b + n * kb, u_width, -2e-5, 0.0);
}
"#;

const LINE_FS: &str = r#"
uniform int u_hidden;
uniform float u_hidden_alpha;
in vec4 v_color;
flat in uint v_id;
noperspective in float v_along;
flat in float v_dashed;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
void main() {
  // fox_render::dash_gap: 9 px dashes with 3 px gaps; a centerline a 9 px dash, a 3 px gap, a
  // 6 px dash and a 3 px gap
  float along = max(v_along, 0.0);
  if (v_dashed > 1.5) {
    float t = mod(along, 21.0);
    if ((t >= 9.0 && t < 12.0) || t >= 18.0) discard;
  } else if ((v_dashed > 0.5 || u_hidden == 1) && mod(along, 12.0) >= 9.0) {
    discard;
  }
  o_color = u_hidden == 1 ? vec4(v_color.rgb, v_color.a * u_hidden_alpha) : v_color;
  o_id = id_color(v_id);
}
"#;

/// Markers are discs of whole pixels around the pixel holding the point, as on the CPU.
const MARKER_VS: &str = r#"
uniform mat4 u_vp;
uniform vec2 u_size;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec3 a_at;
layout(location = 2) in vec4 a_color;
layout(location = 3) in uint a_id;
layout(location = 4) in float a_r;
layout(location = 5) in float a_hollow;
out vec4 v_color;
flat out uint v_id;
flat out vec2 v_center;
flat out float v_r;
flat out float v_hollow;
void main() {
  v_color = a_color;
  v_id = a_id;
  v_r = a_r;
  v_hollow = a_hollow;
  v_center = vec2(0.0);
  vec4 c = u_vp * vec4(a_at, 1.0);
  if (c.w <= 1e-9) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec3 n = c.xyz / c.w;
  // top-down pixel coordinates, like the CPU frame
  v_center = floor(vec2((n.x + 1.0) * 0.5 * u_size.x, (1.0 - n.y) * 0.5 * u_size.y));
  vec2 s = vec2(v_center.x + 0.5, u_size.y - v_center.y - 0.5) + a_corner * (a_r + 1.0);
  gl_Position = vec4(s / u_size * 2.0 - 1.0, clamp(n.z - 5e-5, -1.0, 1.0), 1.0);
}
"#;

const MARKER_FS: &str = r#"
uniform vec2 u_size;
in vec4 v_color;
flat in uint v_id;
flat in vec2 v_center;
flat in float v_r;
flat in float v_hollow;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
void main() {
  vec2 p = vec2(floor(gl_FragCoord.x), u_size.y - 1.0 - floor(gl_FragCoord.y));
  vec2 d = p - v_center;
  if (dot(d, d) > v_r * v_r) discard;
  // a hollow marker's inside is white; its ring is the outermost pixel of the disc, as on the CPU
  o_color = (v_hollow > 0.5 && dot(d, d) <= (v_r - 1.0) * (v_r - 1.0)) ? vec4(1.0, 1.0, 1.0, v_color.a) : v_color;
  o_id = id_color(v_id);
}
"#;

/// A triangle covering the viewport, from `gl_VertexID` alone.
pub(crate) const FULLSCREEN_VS: &str = r#"
out vec2 v_uv;
void main() {
  vec2 p = vec2(float((gl_VertexID & 1) * 4 - 1), float((gl_VertexID & 2) * 2 - 1));
  v_uv = p * 0.5 + 0.5;
  gl_Position = vec4(p, 0.0, 1.0);
}
"#;

/// The background gradient, or Environment Dome's sky over its ground (`fox_render::Dome`).
const BACKGROUND_FS: &str = r#"
uniform vec2 u_size;
uniform vec4 u_top;
uniform vec4 u_bottom;
uniform int u_dome;
uniform vec4 u_zenith;
uniform vec4 u_horizon;
uniform vec4 u_ground;
uniform vec3 u_right;
uniform vec3 u_up;
uniform vec3 u_back;
uniform float u_tan;
in vec2 v_uv;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
void main() {
  float y = u_size.y - gl_FragCoord.y;
  float t = (y - 0.5) / max(u_size.y - 1.0, 1.0);
  o_color = mix(u_top, u_bottom, clamp(t, 0.0, 1.0));
  if (u_dome == 1) {
    float sx = gl_FragCoord.x / u_size.x * 2.0 - 1.0;
    float sy = 1.0 - y / u_size.y * 2.0;
    vec3 d = normalize(-u_back + u_right * (sx * u_tan * u_size.x / u_size.y) + u_up * (sy * u_tan));
    float e = clamp(d.z, -1.0, 1.0);
    vec4 c = e >= 0.0 ? mix(u_horizon, u_zenith, sqrt(e)) : mix(u_horizon, u_ground, min(-e * 6.0, 1.0));
    o_color = floor(c * 255.0 + 0.5) / 255.0;
  }
  o_id = vec4(0.0);
}
"#;

const COMPOSITE_FS: &str = r#"
uniform sampler2D u_tex;
uniform vec4 u_tint;
in vec2 v_uv;
layout(location = 0) out vec4 o_color;
void main() {
  o_color = vec4(texture(u_tex, v_uv).rgb * u_tint.rgb, 1.0);
}
"#;

/// Pick ids as the bytes of an RGBA8 attachment (alpha 1, so blending leaves them intact).
pub(crate) const ID_COLOR: &str = r#"
vec4 id_color(uint id) {
  return vec4(float(id & 255u), float((id >> 8) & 255u), float((id >> 16) & 255u), 255.0) / 255.0;
}
"#;

pub(crate) type Gl = glow::Context;

pub(crate) struct Prog {
  pub(crate) program: glow::Program,
  uniforms: Vec<(&'static str, Option<glow::UniformLocation>)>,
}

impl Prog {
  pub(crate) fn loc(&self, name: &str) -> Option<&glow::UniformLocation> {
    self.uniforms.iter().find(|(n, _)| *n == name).and_then(|(_, l)| l.as_ref())
  }
}

struct GpuMesh {
  vao: glow::VertexArray,
  vbo: glow::Buffer,
  ebo: glow::Buffer,
  chunks: Vec<MeshChunk>,
  /// How many source vertices / indices it was built from (to check a reuse is sound).
  shape: (usize, usize, usize),
  color: [u8; 4],
  highlight: Option<u32>,
  highlight_color: [u8; 4],
  vertex_colors: bool,
  analyze: bool,
  unanalyzed: bool,
  unlit: bool,
  texture: Option<fox_render::MeshTexture>,
}

struct GpuInstances {
  vao: glow::VertexArray,
  vbo: glow::Buffer,
  chunks: [Vec<Bounds>; 4],
  cones: Vec<Cone>,
}

/// What the last draw sent to the GPU, for tests and benchmarks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrawStats {
  /// Mesh chunks drawn, at any level of detail.
  pub chunks: usize,
  /// Mesh chunks left out because they're outside the view.
  pub culled: usize,
  /// Mesh chunks drawn at a level of detail below the full one.
  pub coarse: usize,
  /// Triangles drawn (each mesh counted once, however many passes draw it).
  pub triangles: usize,
  /// Line segments drawn (edges, sketches, the grid), silhouette candidates aside.
  pub segments: usize,
}

/// A camera's view of the scene, for culling chunks and choosing their level of detail.
struct View {
  vp: glam::Mat4,
  /// The view's half-size in pixels, for margins in normalized device coordinates.
  half_px: [f32; 2],
  eye: Vec3,
  persp: bool,
  /// Pixels per world unit: at distance 1 in perspective, everywhere in orthographic.
  px_per_unit: f32,
  culling: bool,
  lod: bool,
}

impl View {
  /// Whether anything in the box can show, `reach` pixels around it included. Conservative: a
  /// box is dropped only when all its corners are beyond one plane of the (widened) frustum.
  fn sees(&self, b: &Bounds, reach: f32) -> bool {
    if !self.culling {
      return true;
    }
    let (mx, my) = (1.0 + reach / self.half_px[0], 1.0 + reach / self.half_px[1]);
    let mut out = [true; 6];
    for k in 0..8 {
      let p = Vec3::new(
        if k & 1 == 0 { b.lo[0] } else { b.hi[0] },
        if k & 2 == 0 { b.lo[1] } else { b.hi[1] },
        if k & 4 == 0 { b.lo[2] } else { b.hi[2] },
      );
      let c = self.vp * p.extend(1.0);
      let beyond = [c.x > c.w * mx, -c.x > c.w * mx, c.y > c.w * my, -c.y > c.w * my, c.z < -c.w, c.z > c.w];
      for (o, b) in out.iter_mut().zip(beyond) {
        *o &= b;
      }
    }
    !out.contains(&true)
  }

  /// The coarsest level whose cells look no wider than [`LOD_PIXELS`] (0 = full detail).
  fn level(&self, c: &MeshChunk) -> usize {
    if !self.lod {
      return 0;
    }
    let scale = if self.persp {
      let b = &c.full;
      let nearest = Vec3::new(
        self.eye.x.clamp(b.lo[0], b.hi[0]),
        self.eye.y.clamp(b.lo[1], b.hi[1]),
        self.eye.z.clamp(b.lo[2], b.hi[2]),
      );
      let d = nearest.distance(self.eye);
      if d <= 0.0 {
        return 0;
      }
      self.px_per_unit / d
    } else {
      self.px_per_unit
    };
    (0..c.lods.len()).rev().find(|&l| c.lods[l].1 > 0.0 && c.lods[l].1 * scale <= LOD_PIXELS).map_or(0, |l| l + 1)
  }

  /// Whether a chunk of fold edges can hold part of the silhouette: some normal in its cone is at
  /// right angles to the direction of the eye from some point of its box. Conservative.
  fn may_outline(&self, b: &Bounds, cone: &Cone, cam: &Camera) -> bool {
    if !self.culling || cone.half >= std::f32::consts::FRAC_PI_2 {
      return true;
    }
    let lo = Vec3::from(b.lo);
    let hi = Vec3::from(b.hi);
    let (to_eye, spread) = if self.persp {
      let center = (lo + hi) / 2.0;
      let (r, d) = ((hi - lo).length() / 2.0, self.eye.distance(center));
      if d <= r * 1.01 {
        return true;
      }
      ((self.eye - center) / d, (r / d).asin())
    } else {
      (cam.basis().2.as_vec3(), 0.0)
    };
    let angle = Vec3::from(cone.axis).dot(to_eye).clamp(-1.0, 1.0).acos();
    (angle - std::f32::consts::FRAC_PI_2).abs() <= cone.half + spread + 1e-3
  }

  /// The parts of a group of instances to draw, as merged runs.
  fn runs(&self, chunks: &[Bounds]) -> Vec<Range<usize>> {
    merged(chunks.iter().filter(|b| self.sees(b, REACH_PX)).map(|b| b.range.clone()))
  }
}

/// Joins ranges that follow on from each other.
fn merged(ranges: impl Iterator<Item = Range<usize>>) -> Vec<Range<usize>> {
  let mut out: Vec<Range<usize>> = Vec::new();
  for r in ranges.filter(|r| !r.is_empty()) {
    match out.last_mut() {
      Some(last) if last.end == r.start => last.end = r.end,
      _ => out.push(r),
    }
  }
  out
}

/// The offscreen frame: colour, pick ids and depth.
struct Target {
  fbo: glow::Framebuffer,
  color: glow::Texture,
  ids: glow::Texture,
  depth: glow::Renderbuffer,
  size: (u32, u32),
}

/// A GPU renderer bound to one OpenGL context.
///
/// [`Renderer::set_scene`] uploads a scene, [`Renderer::draw`] renders it from a camera into an
/// offscreen target, [`Renderer::composite`] puts that image into the current framebuffer and
/// [`Renderer::read_ids`] / [`Renderer::read_rgba`] read it back. Every method must be called
/// with the context it was created with, current on this thread; [`Renderer::destroy`] frees the
/// GPU objects.
pub struct Renderer {
  mesh: Prog,
  line: Prog,
  silhouette: Prog,
  marker: Prog,
  background: Prog,
  composite: Prog,
  /// Corner offsets for the instanced quads (4 vertices of a triangle strip).
  quad: glow::Buffer,
  empty_vao: glow::VertexArray,
  meshes: Vec<GpuMesh>,
  /// The see-through quads drawn with the transparent meshes ([`Renderer::set_quads`]).
  quads: Vec<GpuMesh>,
  lines: Option<GpuInstances>,
  markers: Option<GpuInstances>,
  /// The body surfaces' fold edges, when the style draws silhouettes.
  folds: Option<GpuInstances>,
  /// The outlined meshes' fold edges and the outline's colour ([`fox_render::Mesh::outline`]).
  outlines: Option<(GpuInstances, [u8; 4])>,
  background_colors: [[u8; 4]; 2],
  analysis: Option<Analysis>,
  /// [`Scene::analysis_fade`].
  analysis_fade: f32,
  style: VisualStyle,
  target: Option<Target>,
  /// Leave out what's outside the view (on by default; it never changes the picture).
  pub culling: bool,
  /// The last draw: camera, size, and whether it used levels of detail.
  last: Option<(Camera, u32, u32, bool)>,
  stats: DrawStats,
  /// The GLSL version line the shaders were built with (for the PBR shaders, made on demand).
  header: &'static str,
  /// Physically based shading of the meshes that have a material (the Render workspace).
  pbr: Option<crate::pbr::Pbr>,
  pbr_on: bool,
  /// The effects drawn into the picture ([`Scene::post`]), and their GPU objects once needed.
  post: fox_render::Post,
  fx: Option<crate::effects::Fx>,
  /// Object Shadow's light (see [`fox_render::shadow_light`]), and whether its map needs
  /// drawing again.
  light_vp: Option<glam::Mat4>,
  shadow_dirty: bool,
  /// The meshes' textures ([`fox_render::Mesh::texture`]), by tile key.
  textures: std::collections::HashMap<String, glow::Texture>,
}

fn unit(c: [u8; 4]) -> [f32; 4] {
  c.map(|v| v as f32 / 255.0)
}

impl Renderer {
  /// Compiles the shaders. Fails (and the caller falls back to the CPU rasterizer) without
  /// OpenGL 3.3 or OpenGL ES 3.0, or when a shader doesn't compile.
  pub fn new(gl: &Gl) -> Result<Renderer, String> {
    let v = gl.version();
    let header = if v.is_embedded {
      if v.major < 3 {
        return Err(format!("OpenGL ES {}.{} is too old (3.0 needed)", v.major, v.minor));
      }
      "#version 300 es\nprecision highp float;\nprecision highp int;\n"
    } else {
      if (v.major, v.minor) < (3, 3) {
        return Err(format!("OpenGL {}.{} is too old (3.3 needed)", v.major, v.minor));
      }
      "#version 330 core\n"
    };
    let mvp = ["u_vp", "u_size"];
    let mesh_header = format!("{header}{}", crate::effects::defines());
    let mesh_fs = MESH_FS.replace("OBJ_SHADOW", crate::effects::SHADOW_GLSL).replace("PROJ_UV", PROJ_UV);
    let mesh = program(
      gl,
      &mesh_header,
      MESH_VS,
      &mesh_fs,
      &[
        "u_vp",
        "u_key",
        "u_head",
        "u_fill",
        "u_eye",
        "u_persp",
        "u_color",
        "u_hl_color",
        "u_hl",
        "u_has_hl",
        "u_vcolor",
        "u_mode",
        "u_param",
        "u_param2",
        "u_fade",
        "u_obj_shadow",
        "u_shadow_map",
        "u_light_vp",
        "u_to_light",
        "u_shadow_texel",
        "u_tex_on",
        "u_tex",
        "u_proj",
        "u_tex_xf",
        "u_tex_misc",
        "u_origin",
      ],
    )?;
    let line = program(gl, header, LINE_VS, LINE_FS, &["u_vp", "u_size", "u_hidden", "u_hidden_alpha"])?;
    let silhouette = program(
      gl,
      header,
      SILHOUETTE_VS,
      LINE_FS,
      &[
        "u_vp",
        "u_size",
        "u_hidden",
        "u_hidden_alpha",
        "u_persp",
        "u_eye",
        "u_toward",
        "u_color",
        "u_world_px",
        "u_nudge",
        "u_width",
        "u_crease",
      ],
    )?;
    let marker = program(gl, header, MARKER_VS, MARKER_FS, &mvp)?;
    let background = program(
      gl,
      header,
      FULLSCREEN_VS,
      BACKGROUND_FS,
      &[
        "u_size",
        "u_top",
        "u_bottom",
        "u_dome",
        "u_zenith",
        "u_horizon",
        "u_ground",
        "u_right",
        "u_up",
        "u_back",
        "u_tan",
      ],
    )?;
    let composite = program(gl, header, FULLSCREEN_VS, COMPOSITE_FS, &["u_tex", "u_tint"])?;
    unsafe {
      let quad = gl.create_buffer()?;
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
      let corners: [f32; 8] = [0.0, -1.0, 1.0, -1.0, 0.0, 1.0, 1.0, 1.0];
      gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&corners), glow::STATIC_DRAW);
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
      let empty_vao = gl.create_vertex_array()?;
      let r = Renderer {
        mesh,
        line,
        silhouette,
        marker,
        background,
        composite,
        quad,
        empty_vao,
        meshes: Vec::new(),
        quads: Vec::new(),
        lines: None,
        markers: None,
        folds: None,
        outlines: None,
        background_colors: [[0, 0, 0, 255]; 2],
        analysis: None,
        analysis_fade: 0.0,
        style: VisualStyle::default(),
        target: None,
        culling: true,
        last: None,
        stats: DrawStats::default(),
        header,
        pbr: None,
        pbr_on: false,
        post: fox_render::Post::default(),
        fx: None,
        light_vp: None,
        shadow_dirty: true,
        textures: std::collections::HashMap::new(),
      };
      Ok(r)
    }
  }

  /// Uploads a scene. With `reuse_meshes`, mesh buffers from the previous scene are kept when
  /// the new one has the same meshes (same vertex and index counts): only their colours,
  /// highlights and analysis flags are taken from it. Callers pass it when only the hover
  /// changed, so hovering over a large model doesn't re-upload it.
  pub fn set_scene(&mut self, gl: &Gl, scene: &Scene, reuse_meshes: bool) -> Result<(), String> {
    let same = reuse_meshes
      && self.meshes.len() == scene.meshes.len()
      && self.meshes.iter().zip(&scene.meshes).all(|(g, m)| g.shape == shape(m));
    if !(same && scene.style.silhouettes()) {
      // fold edges depend on the meshes alone: kept while they are, built when a style wants them
      if let Some(f) = self.folds.take() {
        free_instances(gl, f);
      }
    }
    if scene.style.silhouettes() && self.folds.is_none() {
      self.folds = Some(upload_instances(gl, self.quad, &pack::folds(&scene.meshes), FOLD_STRIDE, fold_attribs)?);
    }
    // outlines follow the selection: built afresh each scene (only the outlined meshes')
    if let Some((f, _)) = self.outlines.take() {
      free_instances(gl, f);
    }
    if let Some((inst, color)) = pack::outline_folds(&scene.meshes) {
      self.outlines = Some((upload_instances(gl, self.quad, &inst, FOLD_STRIDE, fold_attribs)?, color));
    }
    if same {
      for (g, m) in self.meshes.iter_mut().zip(&scene.meshes) {
        g.color = m.color;
        g.highlight = m.highlight;
        g.highlight_color = m.highlight_color;
        g.analyze = m.analyze;
        g.unanalyzed = m.unanalyzed;
        g.unlit = m.unlit;
        g.texture = m.texture.clone();
      }
    } else {
      for m in self.meshes.drain(..) {
        free_mesh(gl, m);
      }
      for m in &scene.meshes {
        let g = upload_mesh(gl, m)?;
        self.meshes.push(g);
      }
    }
    self.upload_textures(gl, scene)?;
    if let Some(l) = self.lines.take() {
      free_instances(gl, l);
    }
    if let Some(m) = self.markers.take() {
      free_instances(gl, m);
    }
    self.lines = Some(upload_instances(gl, self.quad, &pack::lines(&scene.lines), LINE_STRIDE, line_attribs)?);
    self.markers =
      Some(upload_instances(gl, self.quad, &pack::markers(&scene.markers), MARKER_STRIDE, marker_attribs)?);
    self.background_colors = [scene.background_top, scene.background_bottom];
    self.analysis = scene.analysis;
    self.analysis_fade = scene.analysis_fade;
    self.style = scene.style;
    self.post = scene.post;
    self.light_vp =
      scene.post.object_shadow.then(|| fox_render::shadow_light(&scene.meshes)).flatten().map(|m| m.as_mat4());
    self.shadow_dirty = true;
    Ok(())
  }

  /// Uploads the scene's textures not on the GPU yet, and frees those it no longer uses.
  fn upload_textures(&mut self, gl: &Gl, scene: &Scene) -> Result<(), String> {
    let used: Vec<&fox_render::Tile> =
      scene.meshes.iter().filter_map(|m| m.texture.as_ref()).map(|t| &*t.tile).collect();
    let gone: Vec<String> = self.textures.keys().filter(|k| !used.iter().any(|t| t.key == **k)).cloned().collect();
    for k in gone {
      if let Some(t) = self.textures.remove(&k) {
        unsafe { gl.delete_texture(t) };
      }
    }
    for t in used {
      if !self.textures.contains_key(&t.key) {
        let tex = upload_tile(gl, t)?;
        self.textures.insert(t.key.clone(), tex);
      }
    }
    Ok(())
  }

  /// The effects' GPU objects, made on first use.
  fn fx(&mut self, gl: &Gl) -> Result<&mut crate::effects::Fx, String> {
    if self.fx.is_none() {
      self.fx = Some(crate::effects::Fx::new(gl, self.header)?);
    }
    self.fx.as_mut().ok_or_else(|| "no effects".to_string())
  }

  /// Object Shadow is drawn this frame (shaded surfaces, a shadow map light).
  fn object_shadow(&self) -> bool {
    self.post.object_shadow && self.style.shaded() && self.light_vp.is_some()
  }

  /// Replaces the see-through quads drawn over the scene ([`fox_render::Quad`]: depth-tested,
  /// blended, no depth or ids written). They are kept apart from the scene, so changing them
  /// (each camera move, for the origin planes) uploads only them.
  pub fn set_quads(&mut self, gl: &Gl, quads: &[fox_render::Quad]) -> Result<(), String> {
    for m in self.quads.drain(..) {
      free_mesh(gl, m);
    }
    for q in quads {
      let g = upload_mesh(gl, &q.mesh())?;
      self.quads.push(g);
    }
    Ok(())
  }

  /// Shades the meshes that `look` gives a material with physically based materials, over its
  /// environment (the Render workspace); `None` goes back to the usual shading. Call it after
  /// [`Renderer::set_scene`], with a material per mesh of that scene.
  pub fn set_look(&mut self, gl: &Gl, look: Option<&crate::pbr::Look>) -> Result<(), String> {
    let Some(look) = look else {
      self.pbr_on = false;
      return Ok(());
    };
    if self.pbr.is_none() {
      self.pbr = Some(crate::pbr::Pbr::new(gl, self.header)?);
    }
    if let Some(p) = self.pbr.as_mut() {
      p.set(gl, look)?;
    }
    self.pbr_on = true;
    Ok(())
  }

  /// The PBR passes, when on and the style shades surfaces.
  fn pbr(&self) -> Option<&crate::pbr::Pbr> {
    self.pbr.as_ref().filter(|_| self.pbr_on && self.style.shaded())
  }

  fn pbr_meshes(&self, runs: &[Vec<Range<usize>>]) -> Vec<crate::pbr::MeshRuns> {
    self.meshes.iter().zip(runs).map(|(m, r)| (m.vao, r.clone(), m.highlight, m.highlight_color)).collect()
  }

  /// Renders the uploaded scene into the offscreen target (resized to `width` × `height`), in the
  /// CPU rasterizer's order: background, opaque meshes, lines and markers, transparent meshes,
  /// then what's drawn on top. Everything is drawn at full detail; chunks outside the view are
  /// left out (see [`Renderer::culling`]), which changes nothing in the picture.
  pub fn draw(&mut self, gl: &Gl, cam: &Camera, width: u32, height: u32) -> Result<(), String> {
    self.draw_view(gl, cam, width, height, false)
  }

  /// [`Renderer::draw`] for a view on the move (orbit, pan, zoom): mesh chunks far enough away
  /// are drawn coarser, at levels of detail whose cells look at most [`LOD_PIXELS`] wide. Picking
  /// stays exact: [`Renderer::read_ids`] redraws the view at full detail first.
  pub fn draw_navigating(&mut self, gl: &Gl, cam: &Camera, width: u32, height: u32) -> Result<(), String> {
    self.draw_view(gl, cam, width, height, true)
  }

  /// What the last draw sent to the GPU.
  pub fn stats(&self) -> DrawStats {
    self.stats
  }

  fn draw_view(&mut self, gl: &Gl, cam: &Camera, width: u32, height: u32, lod: bool) -> Result<(), String> {
    let (w, h) = (width.clamp(1, fox_render::MAX_DIM), height.clamp(1, fox_render::MAX_DIM));
    self.ensure_target(gl, w, h)?;
    self.last = Some((*cam, w, h, lod));
    let vp_mat = cam.view_proj(w, h).as_mat4();
    let view = View {
      vp: vp_mat,
      half_px: [w as f32 / 2.0, h as f32 / 2.0],
      eye: cam.eye().as_vec3(),
      persp: !cam.ortho,
      px_per_unit: if cam.ortho {
        h as f32 / cam.view_height() as f32
      } else {
        h as f32 / (2.0 * (cam.fov / 2.0).tan() as f32)
      },
      culling: self.culling,
      lod,
    };
    let mut stats = DrawStats::default();
    let runs: Vec<Vec<Range<usize>>> = self
      .meshes
      .iter()
      .map(|m| {
        merged(m.chunks.iter().filter_map(|c| {
          if !view.sees(&c.full, 0.0) {
            stats.culled += 1;
            return None;
          }
          let level = view.level(c);
          let r = if level == 0 { c.full.range.clone() } else { c.lods[level - 1].0.clone() };
          stats.chunks += 1;
          stats.coarse += (level > 0) as usize;
          stats.triangles += r.len() / 3;
          Some(r)
        }))
      })
      .collect();
    let vp = vp_mat.to_cols_array();
    let size = [w as f32, h as f32];
    let (right, up, back) = cam.basis();
    let key = (back * 0.8 + up * 0.5 - right * 0.3).normalize().as_vec3();
    let head = back.as_vec3();
    let fill = (right * 0.7 - up * 0.2 + back * 0.3).normalize().as_vec3();
    let eye = cam.eye().as_vec3();
    if self.pbr_on && self.style.shaded() {
      let casters: Vec<_> = self
        .meshes
        .iter()
        .map(|m| {
          let full = merged(m.chunks.iter().map(|c| c.full.range.clone()));
          (m.vao, full, m.analyze && m.color[3] == 255)
        })
        .collect();
      if let Some(p) = self.pbr.as_mut() {
        p.shadow_pass(gl, &casters)?;
      }
    }
    if self.object_shadow() && self.shadow_dirty {
      let casters: Vec<_> = self
        .meshes
        .iter()
        .map(|m| (m.vao, merged(m.chunks.iter().map(|c| c.full.range.clone())), m.analyze && m.color[3] == 255))
        .collect();
      let light = self.light_vp.unwrap_or(glam::Mat4::IDENTITY);
      self.fx(gl)?.shadow_pass(gl, &light, &casters)?;
      self.shadow_dirty = false;
    }
    // the passes over the picture are made before the target is bound
    if self.post.ambient_occlusion || self.post.anti_aliasing {
      self.fx(gl)?;
    }
    let Some(t) = &self.target else { return Err("no render target".into()) };
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
      gl.viewport(0, 0, w as i32, h as i32);
      gl.disable(glow::SCISSOR_TEST);
      gl.disable(glow::CULL_FACE);
      gl.disable(glow::BLEND);
      gl.disable(glow::DITHER);
      gl.color_mask(true, true, true, true);
      gl.depth_mask(true);
      gl.clear_depth_f32(1.0);
      gl.clear(glow::DEPTH_BUFFER_BIT);

      // background (and id 0 everywhere)
      gl.disable(glow::DEPTH_TEST);
      let p = &self.background;
      gl.use_program(Some(p.program));
      gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
      let [top, bottom] = self.background_colors.map(unit);
      gl.uniform_4_f32_slice(p.loc("u_top"), &top);
      gl.uniform_4_f32_slice(p.loc("u_bottom"), &bottom);
      gl.uniform_1_i32(p.loc("u_dome"), self.post.dome.is_some() as i32);
      if let Some(d) = self.post.dome {
        gl.uniform_4_f32_slice(p.loc("u_zenith"), &unit(d.zenith));
        gl.uniform_4_f32_slice(p.loc("u_horizon"), &unit(d.horizon));
        gl.uniform_4_f32_slice(p.loc("u_ground"), &unit(d.ground));
        gl.uniform_3_f32_slice(p.loc("u_right"), &right.as_vec3().to_array());
        gl.uniform_3_f32_slice(p.loc("u_up"), &up.as_vec3().to_array());
        gl.uniform_3_f32_slice(p.loc("u_back"), &back.as_vec3().to_array());
        gl.uniform_1_f32(p.loc("u_tan"), (cam.fov / 2.0).tan() as f32);
      }
      gl.bind_vertex_array(Some(self.empty_vao));
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
      if let Some(pbr) = self.pbr() {
        let inv = cam.view_proj(w, h).inverse().as_mat4();
        pbr.draw_background(gl, &inv, size, self.empty_vao);
      }

      // meshes
      gl.enable(glow::DEPTH_TEST);
      let p = &self.mesh;
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, &vp);
      gl.uniform_3_f32_slice(p.loc("u_key"), &key.to_array());
      gl.uniform_3_f32_slice(p.loc("u_head"), &head.to_array());
      gl.uniform_3_f32_slice(p.loc("u_fill"), &fill.to_array());
      gl.uniform_3_f32_slice(p.loc("u_eye"), &eye.to_array());
      gl.uniform_1_i32(p.loc("u_persp"), !cam.ortho as i32);
      gl.depth_func(glow::LESS);
      let style = self.style;
      let surfaces = style.shaded() || style.line_drawing();
      let bodies = (!surfaces).then_some(false);
      self.draw_meshes(gl, &runs, false, bodies);
      let pview = crate::pbr::MeshView { vp, eye: eye.to_array(), head: head.to_array(), persp: !cam.ortho };
      if let Some(pbr) = self.pbr() {
        pbr.draw_meshes(gl, &self.pbr_meshes(&runs), &pview, false);
      }
      if !surfaces && style.occludes() {
        // wireframe surfaces only hide what's behind them
        gl.color_mask(false, false, false, false);
        self.draw_meshes(gl, &runs, false, Some(true));
        gl.color_mask(true, true, true, true);
      }
      if self.post.ambient_occlusion && surfaces {
        let (fbo, vao) = (t.fbo, self.empty_vao);
        if let Some(fx) = self.fx.as_mut() {
          fx.ambient_occlusion(gl, fbo, (w, h), vao, cam)?;
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.depth_func(glow::LESS);
      }

      // lines and markers blend over what's there; the id attachment keeps its alpha of 1
      gl.enable(glow::BLEND);
      gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ZERO);
      if style.hidden_edges() {
        // the edges behind the surfaces, before any line writes depth
        gl.depth_func(glow::GREATER);
        gl.depth_mask(false);
        self.draw_hidden_edges(gl, &view, cam, size, &vp);
        gl.depth_mask(true);
      }
      gl.depth_func(glow::LEQUAL);
      stats.segments = self.draw_instances(gl, &view, false, size, &vp);
      if style.silhouettes() {
        self.draw_silhouettes(gl, &view, cam, size, &vp, false);
      }
      self.draw_outlines(gl, &view, cam, size, &vp);

      // transparent meshes test against the depth but don't write it
      gl.depth_func(glow::LESS);
      gl.depth_mask(false);
      let p = &self.mesh;
      gl.use_program(Some(p.program));
      self.draw_meshes(gl, &runs, true, bodies);
      if let Some(pbr) = self.pbr() {
        // glass: its reflections added over what's behind it
        gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
        pbr.draw_meshes(gl, &self.pbr_meshes(&runs), &pview, true);
        gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ZERO);
      }
      self.draw_quads(gl);

      // sketch geometry in sketch mode and other on-top overlays
      gl.disable(glow::DEPTH_TEST);
      stats.segments += self.draw_instances(gl, &view, true, size, &vp);
      if self.post.anti_aliasing {
        let (fbo, vao) = (t.fbo, self.empty_vao);
        if let Some(fx) = self.fx.as_mut() {
          fx.anti_alias(gl, fbo, (w, h), vao)?;
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
      }

      gl.depth_mask(true);
      gl.disable(glow::BLEND);
      gl.bind_vertex_array(None);
      gl.use_program(None);
      gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }
    self.stats = stats;
    Ok(())
  }

  /// The opaque or the transparent meshes (the runs of each to draw): with `bodies` set, only
  /// the body surfaces (true) or all but them (false).
  fn draw_meshes(&self, gl: &Gl, runs: &[Vec<Range<usize>>], transparent: bool, bodies: Option<bool>) {
    let p = &self.mesh;
    let paper = self.style.line_drawing();
    unsafe {
      // Object Shadow's map, on texture unit 2
      let shadow_map = self.fx.as_ref().and_then(|f| f.shadow_map()).filter(|_| self.object_shadow());
      if let (Some(tex), Some(light)) = (shadow_map, self.light_vp) {
        gl.active_texture(glow::TEXTURE2);
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.active_texture(glow::TEXTURE0);
        gl.uniform_1_i32(p.loc("u_shadow_map"), 2);
        gl.uniform_matrix_4_f32_slice(p.loc("u_light_vp"), false, &light.to_cols_array());
        gl.uniform_3_f32_slice(p.loc("u_to_light"), &fox_render::to_light().to_array());
        gl.uniform_1_f32(p.loc("u_shadow_texel"), fox_render::shadow_texel(&light.as_dmat4()));
      }
      let pbr = self.pbr();
      let wanted = |i: usize, m: &GpuMesh| {
        (m.color[3] < 255) == transparent && bodies.is_none_or(|b| m.analyze == b) && !pbr.is_some_and(|p| p.has(i))
      };
      for (_, (m, runs)) in self.meshes.iter().zip(runs).enumerate().filter(|(i, (m, _))| wanted(*i, m)) {
        if runs.is_empty() {
          continue;
        }
        let (mode, param, param2) = match self.analysis.filter(|_| m.analyze && !m.unanalyzed) {
          _ if paper && m.analyze => (4, [0.0; 4], [0.0; 4]),
          // unlit in its own colour (a sketch profile)
          _ if m.unlit => (5, [0.0; 4], [0.0; 4]),
          None => (0, [0.0; 4], [0.0; 4]),
          Some(Analysis::Zebra { stripes, vertical }) => (1, [stripes, vertical as u8 as f32, 0.0, 0.0], [0.0; 4]),
          Some(Analysis::Draft { pull, lo, hi, tol }) => {
            let d = Vec3::from(pull).normalize_or_zero();
            (2, [d.x, d.y, d.z, 0.0], [lo, hi, tol, 0.0])
          }
          Some(Analysis::Accessibility { dir }) => {
            let d = Vec3::from(dir).normalize_or_zero();
            (3, [d.x, d.y, d.z, 0.0], [0.0; 4])
          }
          Some(Analysis::Environment { rotate, mirror, sharp }) => {
            (6, [rotate, mirror as u8 as f32, sharp as u8 as f32, 0.0], [0.0; 4])
          }
        };
        let color = if mode == 4 {
          let p = fox_render::PAPER;
          [p[0], p[1], p[2], m.color[3]]
        } else {
          m.color
        };
        gl.uniform_4_f32_slice(p.loc("u_color"), &unit(color));
        gl.uniform_4_f32_slice(p.loc("u_hl_color"), &unit(m.highlight_color));
        gl.uniform_1_u32(p.loc("u_hl"), m.highlight.unwrap_or(0));
        gl.uniform_1_i32(p.loc("u_has_hl"), m.highlight.is_some() as i32);
        gl.uniform_1_i32(p.loc("u_vcolor"), (m.vertex_colors && mode != 4) as i32);
        gl.uniform_1_i32(p.loc("u_mode"), mode);
        gl.uniform_4_f32_slice(p.loc("u_param"), &param);
        gl.uniform_4_f32_slice(p.loc("u_param2"), &param2);
        gl.uniform_1_f32(p.loc("u_fade"), self.analysis_fade);
        let shadowed = shadow_map.is_some() && m.analyze && m.color[3] == 255;
        gl.uniform_1_i32(p.loc("u_obj_shadow"), shadowed as i32);
        let tex = m.texture.as_ref().filter(|_| mode != 4).and_then(|t| Some((t, *self.textures.get(&t.tile.key)?)));
        match tex {
          Some((t, tex)) => {
            let pr = &t.projection;
            let (s, c) = pr.rotation.to_radians().sin_cos();
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(p.loc("u_tex"), 3);
            gl.uniform_1_i32(p.loc("u_tex_on"), 1);
            gl.uniform_2_i32(p.loc("u_proj"), pr.kind, pr.axis);
            gl.uniform_4_f32_slice(p.loc("u_tex_xf"), &[pr.scale, s, c, 0.0]);
            gl.uniform_3_f32_slice(p.loc("u_tex_misc"), &[pr.offset[0], pr.offset[1], 0.0]);
            gl.uniform_3_f32_slice(p.loc("u_origin"), &pr.origin);
          }
          None => gl.uniform_1_i32(p.loc("u_tex_on"), 0),
        }
        gl.bind_vertex_array(Some(m.vao));
        for r in runs {
          gl.draw_elements(glow::TRIANGLES, r.len() as i32, glow::UNSIGNED_INT, (r.start * 4) as i32);
        }
      }
    }
  }

  /// The see-through quads, unlit in their colours, into the colour attachment only (the ids
  /// behind them stay pickable), with the mesh program's view uniforms as the draw set them.
  fn draw_quads(&self, gl: &Gl) {
    if self.quads.is_empty() {
      return;
    }
    let p = &self.mesh;
    unsafe {
      gl.use_program(Some(p.program));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
      gl.uniform_1_i32(p.loc("u_has_hl"), 0);
      gl.uniform_1_i32(p.loc("u_vcolor"), 0);
      gl.uniform_1_i32(p.loc("u_mode"), 5);
      gl.uniform_1_i32(p.loc("u_tex_on"), 0);
      gl.uniform_4_f32_slice(p.loc("u_param"), &[0.0; 4]);
      for m in &self.quads {
        gl.uniform_4_f32_slice(p.loc("u_color"), &unit(m.color));
        gl.bind_vertex_array(Some(m.vao));
        for c in &m.chunks {
          let r = &c.full.range;
          gl.draw_elements(glow::TRIANGLES, r.len() as i32, glow::UNSIGNED_INT, (r.start * 4) as i32);
        }
      }
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
    }
  }

  /// Lines, then markers, of one layer (`on_top` or not), the chunks in view. Things without a
  /// pick id leave the id attachment alone, as the CPU path leaves `Frame::ids`. Returns the
  /// line segments drawn.
  fn draw_instances(&self, gl: &Gl, view: &View, on_top: bool, size: [f32; 2], vp: &[f32; 16]) -> usize {
    let kinds = [
      (&self.line, &self.lines, LINE_STRIDE, line_attribs as AttribFn),
      (&self.marker, &self.markers, MARKER_STRIDE, marker_attribs),
    ];
    let mut segments = 0;
    unsafe {
      for (p, inst, stride, attribs) in kinds {
        let Some(inst) = inst else { continue };
        gl.use_program(Some(p.program));
        gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, vp);
        gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
        gl.bind_vertex_array(Some(inst.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst.vbo));
        for has_id in [false, true] {
          let runs = view.runs(&inst.chunks[on_top as usize * 2 + has_id as usize]);
          if runs.is_empty() {
            continue;
          }
          if has_id {
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
          } else {
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
          }
          for r in runs {
            if stride == LINE_STRIDE {
              segments += r.len();
            }
            attribs(gl, (r.start * stride * 4) as i32);
            gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, r.len() as i32);
          }
        }
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
      }
    }
    segments
  }

  /// The depth-tested edges (lines with pick ids) and silhouettes as hidden edges, with the depth
  /// test the caller set; colour only.
  fn draw_hidden_edges(&self, gl: &Gl, view: &View, cam: &Camera, size: [f32; 2], vp: &[f32; 16]) {
    let alpha = self.style.hidden_alpha();
    if let (p, Some(inst)) = (&self.line, &self.lines) {
      let runs = view.runs(&inst.chunks[1]);
      unsafe {
        gl.use_program(Some(p.program));
        gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, vp);
        gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
        gl.uniform_1_i32(p.loc("u_hidden"), 1);
        gl.uniform_1_f32(p.loc("u_hidden_alpha"), alpha);
        gl.bind_vertex_array(Some(inst.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst.vbo));
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
        for r in runs {
          line_attribs(gl, (r.start * LINE_STRIDE * 4) as i32);
          gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, r.len() as i32);
        }
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
        gl.uniform_1_i32(p.loc("u_hidden"), 0);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
      }
    }
    if self.style.silhouettes() {
      self.draw_silhouettes(gl, view, cam, size, vp, true);
    }
  }

  /// The silhouettes of the body surfaces: visible ones as edges, or (`hidden`) the parts behind
  /// the surfaces, faint and dashed, without ids.
  fn draw_silhouettes(&self, gl: &Gl, view: &View, cam: &Camera, size: [f32; 2], vp: &[f32; 16], hidden: bool) {
    let (p, Some(inst)) = (&self.silhouette, &self.folds) else { return };
    let chunks = inst.chunks[1].iter().zip(&inst.cones);
    let runs = merged(
      chunks.filter(|(b, c)| view.sees(b, REACH_PX) && view.may_outline(b, c, cam)).map(|(b, _)| b.range.clone()),
    );
    if runs.is_empty() {
      return;
    }
    let toward = cam.basis().2.as_vec3();
    unsafe {
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, vp);
      gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
      gl.uniform_1_i32(p.loc("u_hidden"), hidden as i32);
      gl.uniform_1_f32(p.loc("u_hidden_alpha"), self.style.hidden_alpha());
      gl.uniform_1_i32(p.loc("u_persp"), !cam.ortho as i32);
      gl.uniform_3_f32_slice(p.loc("u_eye"), &view.eye.to_array());
      gl.uniform_3_f32_slice(p.loc("u_toward"), &toward.to_array());
      gl.uniform_4_f32_slice(p.loc("u_color"), &unit(fox_render::SILHOUETTE_COLOR));
      let per_px = cam.view_height() as f32 / cam.distance as f32 / size[1];
      let world_px = if cam.ortho { per_px * cam.distance as f32 } else { per_px };
      gl.uniform_1_f32(p.loc("u_world_px"), world_px);
      gl.uniform_1_f32(p.loc("u_nudge"), fox_render::SILHOUETTE_NUDGE_PX);
      gl.uniform_1_f32(p.loc("u_width"), 1.0);
      gl.uniform_1_f32(p.loc("u_crease"), -2.0);
      gl.bind_vertex_array(Some(inst.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst.vbo));
      if hidden {
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
      }
      for r in runs {
        fold_attribs(gl, (r.start * FOLD_STRIDE * 4) as i32);
        gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, r.len() as i32);
      }
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
    }
  }

  /// The outlined meshes' silhouettes and creases, in the outline's colour, whatever the style
  /// (see [`fox_render::outline_lines`]).
  fn draw_outlines(&self, gl: &Gl, view: &View, cam: &Camera, size: [f32; 2], vp: &[f32; 16]) {
    let (p, Some((inst, color))) = (&self.silhouette, &self.outlines) else { return };
    // creases show from any side: no cone culling
    let runs = merged(inst.chunks[1].iter().filter(|b| view.sees(b, REACH_PX)).map(|b| b.range.clone()));
    if runs.is_empty() {
      return;
    }
    let toward = cam.basis().2.as_vec3();
    unsafe {
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, vp);
      gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
      gl.uniform_1_i32(p.loc("u_hidden"), 0);
      gl.uniform_1_f32(p.loc("u_hidden_alpha"), 1.0);
      gl.uniform_1_i32(p.loc("u_persp"), !cam.ortho as i32);
      gl.uniform_3_f32_slice(p.loc("u_eye"), &view.eye.to_array());
      gl.uniform_3_f32_slice(p.loc("u_toward"), &toward.to_array());
      gl.uniform_4_f32_slice(p.loc("u_color"), &unit(*color));
      let per_px = cam.view_height() as f32 / cam.distance as f32 / size[1];
      let world_px = if cam.ortho { per_px * cam.distance as f32 } else { per_px };
      gl.uniform_1_f32(p.loc("u_world_px"), world_px);
      gl.uniform_1_f32(p.loc("u_nudge"), fox_render::SILHOUETTE_NUDGE_PX);
      gl.uniform_1_f32(p.loc("u_width"), fox_render::OUTLINE_WIDTH);
      gl.uniform_1_f32(p.loc("u_crease"), fox_render::OUTLINE_CREASE_COS);
      gl.bind_vertex_array(Some(inst.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst.vbo));
      for r in runs {
        fold_attribs(gl, (r.start * FOLD_STRIDE * 4) as i32);
        gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, r.len() as i32);
      }
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
    }
  }

  fn ensure_target(&mut self, gl: &Gl, w: u32, h: u32) -> Result<(), String> {
    if self.target.as_ref().is_some_and(|t| t.size == (w, h)) {
      return Ok(());
    }
    if let Some(t) = self.target.take() {
      free_target(gl, t);
    }
    unsafe {
      let tex = |filter: u32| -> Result<glow::Texture, String> {
        let t = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
          glow::TEXTURE_2D,
          0,
          glow::RGBA8 as i32,
          w as i32,
          h as i32,
          0,
          glow::RGBA,
          glow::UNSIGNED_BYTE,
          glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        Ok(t)
      };
      let color = tex(glow::LINEAR)?;
      let ids = tex(glow::NEAREST)?;
      gl.bind_texture(glow::TEXTURE_2D, None);
      let depth = gl.create_renderbuffer()?;
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
      gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT32F, w as i32, h as i32);
      gl.bind_renderbuffer(glow::RENDERBUFFER, None);
      let fbo = gl.create_framebuffer()?;
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT1, glow::TEXTURE_2D, Some(ids), 0);
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, None);
      let t = Target { fbo, color, ids, depth, size: (w, h) };
      if status != glow::FRAMEBUFFER_COMPLETE {
        free_target(gl, t);
        return Err(format!("the offscreen framebuffer is incomplete (status {status:#x})"));
      }
      self.target = Some(t);
    }
    Ok(())
  }

  /// Size of the last drawn image.
  pub fn size(&self) -> Option<(u32, u32)> {
    self.target.as_ref().map(|t| t.size)
  }

  /// Draws the last image stretched over `viewport` (x, y from the bottom, width, height in
  /// pixels) of `target` (`None` = the default framebuffer), clipped to `scissor` and multiplied
  /// by `tint`. Leaves the state egui's painter expects: scissor on, depth test off, blending on.
  pub fn composite(
    &self,
    gl: &Gl,
    target: Option<glow::Framebuffer>,
    viewport: [i32; 4],
    scissor: [i32; 4],
    tint: [f32; 4],
  ) {
    let Some(t) = &self.target else { return };
    let p = &self.composite;
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, target);
      gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
      gl.enable(glow::SCISSOR_TEST);
      gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
      gl.disable(glow::DEPTH_TEST);
      gl.disable(glow::BLEND);
      gl.use_program(Some(p.program));
      gl.active_texture(glow::TEXTURE0);
      gl.bind_texture(glow::TEXTURE_2D, Some(t.color));
      gl.uniform_1_i32(p.loc("u_tex"), 0);
      gl.uniform_4_f32_slice(p.loc("u_tint"), &tint);
      gl.bind_vertex_array(Some(self.empty_vao));
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
      gl.bind_vertex_array(None);
      gl.bind_texture(glow::TEXTURE_2D, None);
      gl.use_program(None);
      gl.enable(glow::BLEND);
    }
  }

  fn read(&self, gl: &Gl, attachment: u32) -> Option<(u32, u32, Vec<u8>)> {
    let t = self.target.as_ref()?;
    let (w, h) = t.size;
    let mut px = vec![0u8; (w * h * 4) as usize];
    unsafe {
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(t.fbo));
      gl.read_buffer(attachment);
      gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
      gl.read_pixels(
        0,
        0,
        w as i32,
        h as i32,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut px)),
      );
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
    }
    // OpenGL rows run bottom-up; frames run top-down
    let row = (w * 4) as usize;
    let mut out = Vec::with_capacity(px.len());
    for r in px.chunks_exact(row).rev() {
      out.extend_from_slice(r);
    }
    Some((w, h, out))
  }

  /// The pick ids of the last image as a frame (`rgba` and `depth` left empty), for
  /// [`Frame::pick`] and window selection.
  /// After [`Renderer::draw_navigating`] the view is drawn again at full detail first, so the
  /// ids are always exact.
  pub fn read_ids(&mut self, gl: &Gl) -> Option<Frame> {
    if let Some((cam, w, h, true)) = self.last {
      self.draw(gl, &cam, w, h).ok()?;
    }
    let (w, h, px) = self.read(gl, glow::COLOR_ATTACHMENT1)?;
    let ids = px.chunks_exact(4).map(|p| u32::from_le_bytes([p[0], p[1], p[2], 0])).collect();
    Some(Frame { width: w, height: h, rgba: Vec::new(), depth: Vec::new(), ids })
  }

  /// The last image, top-down RGBA.
  pub fn read_rgba(&self, gl: &Gl) -> Option<Vec<u8>> {
    let (_, _, mut px) = self.read(gl, glow::COLOR_ATTACHMENT0)?;
    for p in px.chunks_exact_mut(4) {
      p[3] = 255;
    }
    Some(px)
  }

  /// Renders a scene into a complete [`Frame`] (colour and ids), like [`fox_render::render`].
  pub fn render(&mut self, gl: &Gl, scene: &Scene, cam: &Camera, width: u32, height: u32) -> Result<Frame, String> {
    self.set_scene(gl, scene, false)?;
    self.draw(gl, cam, width, height)?;
    let mut f = self.read_ids(gl).ok_or("nothing drawn")?;
    f.rgba = self.read_rgba(gl).ok_or("nothing drawn")?;
    f.depth = vec![f32::INFINITY; f.ids.len()];
    debug_assert!(f.ids.iter().all(|&i| i == NO_ID || i < crate::MAX_ID));
    Ok(f)
  }

  /// Frees every GPU object.
  pub fn destroy(mut self, gl: &Gl) {
    for m in self.meshes.drain(..).chain(self.quads.drain(..)) {
      free_mesh(gl, m);
    }
    let outlines = self.outlines.take().map(|o| o.0);
    for i in [self.lines.take(), self.markers.take(), self.folds.take(), outlines].into_iter().flatten() {
      free_instances(gl, i);
    }
    if let Some(t) = self.target.take() {
      free_target(gl, t);
    }
    if let Some(p) = self.pbr.take() {
      p.destroy(gl);
    }
    if let Some(f) = self.fx.take() {
      f.destroy(gl);
    }
    unsafe {
      for p in [&self.mesh, &self.line, &self.silhouette, &self.marker, &self.background, &self.composite] {
        gl.delete_program(p.program);
      }
      gl.delete_buffer(self.quad);
      gl.delete_vertex_array(self.empty_vao);
      for (_, t) in self.textures.drain() {
        gl.delete_texture(t);
      }
    }
  }
}

fn shape(m: &fox_render::Mesh) -> (usize, usize, usize) {
  (m.positions.len(), m.indices.len(), m.tri_ids.len())
}

pub(crate) fn program(gl: &Gl, header: &str, vs: &str, fs: &str, uniforms: &[&'static str]) -> Result<Prog, String> {
  unsafe {
    let program = gl.create_program()?;
    let mut shaders = Vec::new();
    for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
      let s = gl.create_shader(kind)?;
      gl.shader_source(s, &format!("{header}{}", src.replace("LINE_QUAD", LINE_QUAD).replace("ID_COLOR", ID_COLOR)));
      gl.compile_shader(s);
      if !gl.get_shader_compile_status(s) {
        let log = gl.get_shader_info_log(s);
        gl.delete_shader(s);
        gl.delete_program(program);
        return Err(format!("shader compile error: {log}"));
      }
      gl.attach_shader(program, s);
      shaders.push(s);
    }
    gl.link_program(program);
    for s in shaders {
      gl.detach_shader(program, s);
      gl.delete_shader(s);
    }
    if !gl.get_program_link_status(program) {
      let log = gl.get_program_info_log(program);
      gl.delete_program(program);
      return Err(format!("shader link error: {log}"));
    }
    let uniforms = uniforms.iter().map(|n| (*n, gl.get_uniform_location(program, n))).collect();
    Ok(Prog { program, uniforms })
  }
}

fn upload_mesh(gl: &Gl, m: &fox_render::Mesh) -> Result<GpuMesh, String> {
  let d = pack::mesh(m);
  let stride = (MESH_STRIDE * 4) as i32;
  unsafe {
    let vao = gl.create_vertex_array()?;
    let vbo = gl.create_buffer()?;
    let ebo = gl.create_buffer()?;
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&d.verts), glow::STATIC_DRAW);
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck::cast_slice(&d.indices), glow::STATIC_DRAW);
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, stride, 24);
    gl.enable_vertex_attrib_array(3);
    gl.vertex_attrib_pointer_i32(3, 1, glow::UNSIGNED_INT, stride, 28);
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
    Ok(GpuMesh {
      vao,
      vbo,
      ebo,
      chunks: d.chunks,
      shape: shape(m),
      color: m.color,
      highlight: m.highlight,
      highlight_color: m.highlight_color,
      vertex_colors: !m.colors.is_empty(),
      analyze: m.analyze,
      unanalyzed: m.unanalyzed,
      unlit: m.unlit,
      texture: m.texture.clone(),
    })
  }
}

/// A texture tile as an RGBA8 texture that repeats, sampled as stored (sRGB, like the mesh
/// colours the shader works in).
fn upload_tile(gl: &Gl, t: &fox_render::Tile) -> Result<glow::Texture, String> {
  if t.width == 0 || t.height == 0 || t.rgba.len() < (t.width * t.height * 4) as usize {
    return Err(format!("texture '{}' has no texels", t.key));
  }
  unsafe {
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
    gl.tex_image_2d(
      glow::TEXTURE_2D,
      0,
      glow::RGBA8 as i32,
      t.width as i32,
      t.height as i32,
      0,
      glow::RGBA,
      glow::UNSIGNED_BYTE,
      glow::PixelUnpackData::Slice(Some(&t.rgba)),
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

fn free_mesh(gl: &Gl, m: GpuMesh) {
  unsafe {
    gl.delete_vertex_array(m.vao);
    gl.delete_buffer(m.vbo);
    gl.delete_buffer(m.ebo);
  }
}

/// Points a VAO's per-instance attributes at the instance data, starting `offset` bytes in.
type AttribFn = fn(&Gl, i32);

fn line_attribs(gl: &Gl, offset: i32) {
  let s = (LINE_STRIDE * 4) as i32;
  unsafe {
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, s, offset);
    gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, s, offset + 12);
    gl.vertex_attrib_pointer_f32(3, 4, glow::UNSIGNED_BYTE, true, s, offset + 24);
    gl.vertex_attrib_pointer_i32(4, 1, glow::UNSIGNED_INT, s, offset + 28);
    gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, s, offset + 32);
  }
}

fn fold_attribs(gl: &Gl, offset: i32) {
  let s = (FOLD_STRIDE * 4) as i32;
  unsafe {
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, s, offset);
    gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, s, offset + 12);
    gl.vertex_attrib_pointer_f32(3, 3, glow::FLOAT, false, s, offset + 24);
    gl.vertex_attrib_pointer_f32(4, 3, glow::FLOAT, false, s, offset + 36);
    gl.vertex_attrib_pointer_i32(5, 2, glow::UNSIGNED_INT, s, offset + 48);
  }
}

fn marker_attribs(gl: &Gl, offset: i32) {
  let s = (MARKER_STRIDE * 4) as i32;
  unsafe {
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, s, offset);
    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, s, offset + 12);
    gl.vertex_attrib_pointer_i32(3, 1, glow::UNSIGNED_INT, s, offset + 16);
    gl.vertex_attrib_pointer_f32(4, 1, glow::FLOAT, false, s, offset + 20);
    gl.vertex_attrib_pointer_f32(5, 1, glow::FLOAT, false, s, offset + 24);
  }
}

fn upload_instances(
  gl: &Gl,
  quad: glow::Buffer,
  inst: &pack::Instances,
  stride: usize,
  attribs: AttribFn,
) -> Result<GpuInstances, String> {
  let n_attribs = 5;
  unsafe {
    let vao = gl.create_vertex_array()?;
    let vbo = gl.create_buffer()?;
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&inst.data), glow::STATIC_DRAW);
    for a in 1..=n_attribs {
      gl.enable_vertex_attrib_array(a);
      gl.vertex_attrib_divisor(a, 1);
    }
    attribs(gl, 0);
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    debug_assert_eq!(inst.data.len() % stride, 0);
    Ok(GpuInstances { vao, vbo, chunks: inst.chunks.clone(), cones: inst.cones.clone() })
  }
}

fn free_instances(gl: &Gl, i: GpuInstances) {
  unsafe {
    gl.delete_vertex_array(i.vao);
    gl.delete_buffer(i.vbo);
  }
}

fn free_target(gl: &Gl, t: Target) {
  unsafe {
    gl.delete_framebuffer(t.fbo);
    gl.delete_texture(t.color);
    gl.delete_texture(t.ids);
    gl.delete_renderbuffer(t.depth);
  }
}
